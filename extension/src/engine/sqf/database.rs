// SQF command database — Arma 3 command registry.
//
// Primary source: arma3-wiki crate — tries remote git on startup (6-hour cache),
// falls back to build-time embedded data. Covers ~2,700 Arma 3 commands.
// Each command stores arity, return type classification, and wiki groups.

use std::collections::HashMap;
use std::sync::OnceLock;

use arma3_wiki::model::Call;

/// Command arity classification.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Arity {
    Nular,
    Unary,
    Binary,
}

/// Simplified return type for SQF dispatch decisions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ReturnType {
    Number,
    String,
    Boolean,
    Array,
    Nothing,
    /// Game engine types we can't produce in Rust (Object, Config, Code, …)
    Other,
}

impl ReturnType {
    fn from_wiki_value(v: &arma3_wiki::model::Value) -> Self {
        use arma3_wiki::model::Value;
        match v {
            Value::Number | Value::NumberEnum(_) | Value::NumberRange(..) => ReturnType::Number,
            Value::String | Value::StringEnum(_) | Value::StructuredText => ReturnType::String,
            Value::Boolean => ReturnType::Boolean,
            Value::ArraySized { .. }
            | Value::ArrayUnknown
            | Value::ArrayUnsized { .. }
            | Value::ArrayEmpty
            | Value::ArrayDate
            | Value::ArrayColor
            | Value::ArrayColorRgb
            | Value::ArrayColorRgba
            | Value::ArrayEdenEntities
            | Value::Position
            | Value::Position2d
            | Value::Position3d
            | Value::Position3dASL
            | Value::Position3DASLW
            | Value::Position3dATL
            | Value::Position3dAGL
            | Value::Position3dAGLS
            | Value::Position3dRelative
            | Value::Vector
            | Value::Vector2d
            | Value::Vector3d
            | Value::TurretPath
            | Value::UnitLoadoutArray => ReturnType::Array,
            Value::Nothing => ReturnType::Nothing,
            Value::Anything => ReturnType::Other,
            _ => ReturnType::Other,
        }
    }

    /// Check if this return type is actually "implementable" in Rust evaluation
    /// or requires the game engine.
    pub(crate) fn is_implementable(self) -> bool {
        matches!(
            self,
            ReturnType::Number | ReturnType::String | ReturnType::Boolean | ReturnType::Array | ReturnType::Nothing
        )
    }
}

/// Metadata stored per command.
#[derive(Debug, Clone)]
pub(crate) struct CmdInfo {
    pub arity: Arity,
    pub ret: ReturnType,
    pub groups: Vec<String>,
}

/// Database metadata.
#[derive(Debug, Clone)]
pub(crate) struct WikiMeta {
    pub source: &'static str,
    pub major: u8,
    pub minor: u8,
    pub command_count: usize,
}

struct Database {
    commands: HashMap<String, CmdInfo>,
    meta: WikiMeta,
}

impl Database {
    fn load() -> Self {
        let mut commands: HashMap<String, CmdInfo> = HashMap::new();

        // Load arma3-wiki data FIRST (primary source)
        let wiki = std::panic::catch_unwind(|| arma3_wiki::Wiki::load(false)).ok();
        let meta = match &wiki {
            Some(w) => {
                let v = w.version();
                let source = if w.updated() { "git" } else { "cache" };
                let n = w.commands().iter().count();

                for (name, cmd) in w.commands().iter() {
                    let groups: Vec<String> = cmd.groups().to_vec();
                    // Pick the "best" syntax (prefer nular > unary > binary, prefer Number return)
                    let mut best: Option<(Arity, ReturnType)> = None;
                    for syn in cmd.syntax() {
                        let arity = match syn.call() {
                            Call::Nular => Arity::Nular,
                            Call::Unary(_) => Arity::Unary,
                            Call::Binary(_, _) => Arity::Binary,
                        };
                        let ret = ReturnType::from_wiki_value(syn.ret().typ());
                        match best {
                            None => best = Some((arity, ret)),
                            Some((existing_arity, existing_ret)) => {
                                let better_arity = matches!(
                                    (existing_arity, &arity),
                                    (Arity::Binary, Arity::Unary)
                                        | (Arity::Binary, Arity::Nular)
                                        | (Arity::Unary, Arity::Nular)
                                );
                                let better_ret = existing_ret == ReturnType::Other && ret != ReturnType::Other;
                                if better_arity || better_ret {
                                    best = Some((arity, ret));
                                }
                            }
                        }
                    }
                    if let Some((arity, ret)) = best {
                        commands.insert(name.clone(), CmdInfo { arity, ret, groups });
                    }
                }

                WikiMeta {
                    source,
                    major: v.major(),
                    minor: v.minor(),
                    command_count: n,
                }
            }
            None => WikiMeta {
                source: "static",
                major: 0,
                minor: 0,
                command_count: 0,
            },
        };

        // Ensure native commands with Rust implementations are present.
        // Runs regardless of whether wiki data loaded successfully.
        let n0 = commands.len();
        for &(name, _) in crate::engine::sqf::commands::NATIVE_CMD_FNS {
            commands.entry(name.to_string()).or_insert_with(|| {
                // Use correct metadata from fallback static
                let (arity, ret) = NATIVE_META
                    .iter()
                    .find(|&&(n, _, _)| n == name)
                    .map(|&(_, a, r)| (a, r))
                    .unwrap_or((Arity::Unary, ReturnType::Other));
                CmdInfo {
                    arity,
                    ret,
                    groups: vec!["Native".into()],
                }
            });
        }

        eprintln!(
            "[a3sql] SQF DB: {} commands (+{} wiki, +{} native-only)",
            commands.len(),
            n0,
            commands.len() - n0,
        );

        // Version drift check
        if let Some(cfg) = crate::config::Config::load().game_version {
            let wiki_v = format!("{}.{}", meta.major, meta.minor);
            if wiki_v != cfg {
                eprintln!(
                    "[a3sql] WARNING: wiki data v{} differs from configured game_version {}",
                    wiki_v, cfg,
                );
            }
        }

        Database { commands, meta }
    }

    fn lookup(&self, name: &str) -> Option<&CmdInfo> {
        self.commands.get(name)
    }
}

fn global_db() -> &'static Database {
    static DB: OnceLock<Database> = OnceLock::new();
    DB.get_or_init(Database::load)
}

/// Look up a command by name (lowercased) and return its metadata.
pub(crate) fn lookup_info(name: &str) -> Option<&'static CmdInfo> {
    global_db().lookup(name)
}

/// Look up just the arity.
pub(crate) fn lookup(name: &str) -> Option<Arity> {
    global_db().lookup(name).map(|i| i.arity)
}

/// Check if a name is a known command.
pub(crate) fn is_command(name: &str) -> bool {
    global_db().lookup(name).is_some()
}

/// Wiki metadata.
pub(crate) fn wiki_meta() -> &'static WikiMeta {
    &global_db().meta
}

// ── Native fallback metadata ──────────────────────────────────────────
// Correct arity/return-type for every native command, used when wiki data
// is unavailable (e.g. offline CI).

static NATIVE_META: &[(&str, Arity, ReturnType)] = &[
    ("pi", Arity::Nular, ReturnType::Number),
    ("true", Arity::Nular, ReturnType::Boolean),
    ("false", Arity::Nular, ReturnType::Boolean),
    ("nil", Arity::Nular, ReturnType::Nothing),
    ("abs", Arity::Unary, ReturnType::Number),
    ("acos", Arity::Unary, ReturnType::Number),
    ("asin", Arity::Unary, ReturnType::Number),
    ("atan", Arity::Unary, ReturnType::Number),
    ("ceil", Arity::Unary, ReturnType::Number),
    ("cos", Arity::Unary, ReturnType::Number),
    ("deg", Arity::Unary, ReturnType::Number),
    ("exp", Arity::Unary, ReturnType::Number),
    ("floor", Arity::Unary, ReturnType::Number),
    ("ln", Arity::Unary, ReturnType::Number),
    ("log", Arity::Unary, ReturnType::Number),
    ("log10", Arity::Unary, ReturnType::Number),
    ("rad", Arity::Unary, ReturnType::Number),
    ("round", Arity::Unary, ReturnType::Number),
    ("sin", Arity::Unary, ReturnType::Number),
    ("sqrt", Arity::Unary, ReturnType::Number),
    ("tan", Arity::Unary, ReturnType::Number),
    ("cosec", Arity::Unary, ReturnType::Number),
    ("sec", Arity::Unary, ReturnType::Number),
    ("cot", Arity::Unary, ReturnType::Number),
    ("str", Arity::Unary, ReturnType::String),
    ("to_string", Arity::Unary, ReturnType::String),
    ("toupper", Arity::Unary, ReturnType::String),
    ("to_upper", Arity::Unary, ReturnType::String),
    ("tolower", Arity::Unary, ReturnType::String),
    ("to_lower", Arity::Unary, ReturnType::String),
    ("trim", Arity::Unary, ReturnType::String),
    ("typename", Arity::Unary, ReturnType::String),
    ("type_name", Arity::Unary, ReturnType::String),
    ("count", Arity::Unary, ReturnType::Number),
    ("parsenumber", Arity::Unary, ReturnType::Number),
    ("parse_number", Arity::Unary, ReturnType::Number),
    ("random", Arity::Unary, ReturnType::Number),
    ("trunc", Arity::Unary, ReturnType::Number),
    ("sign", Arity::Unary, ReturnType::Number),
    ("hint", Arity::Unary, ReturnType::String),
    ("hintc", Arity::Unary, ReturnType::String),
    ("vectormagnitude", Arity::Unary, ReturnType::Number),
    ("isnil", Arity::Unary, ReturnType::Boolean),
    ("is_null", Arity::Unary, ReturnType::Boolean),
    ("min", Arity::Binary, ReturnType::Number),
    ("max", Arity::Binary, ReturnType::Number),
    ("atan2", Arity::Binary, ReturnType::Number),
    ("clamp", Arity::Binary, ReturnType::Number),
    ("find", Arity::Binary, ReturnType::Number),
    ("select", Arity::Binary, ReturnType::Other),
    ("in", Arity::Binary, ReturnType::Boolean),
    ("replace", Arity::Binary, ReturnType::String),
    ("isequalto", Arity::Binary, ReturnType::Boolean),
    ("is_equal_to", Arity::Binary, ReturnType::Boolean),
    ("pushback", Arity::Binary, ReturnType::Array),
    ("deleteat", Arity::Binary, ReturnType::Array),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wiki_info_basic() {
        let m = wiki_meta();
        eprintln!(
            "[test] arma3-wiki: source={}, v{}.{}, {} commands",
            m.source, m.major, m.minor, m.command_count,
        );
        if m.command_count > 0 {
            assert!(
                m.command_count > 2000,
                "expected 2000+ wiki commands, got {}",
                m.command_count
            );
            assert!(
                m.major > 0 || m.minor > 0,
                "wiki version should be non-zero, got {}.{}",
                m.major,
                m.minor,
            );
        }
    }

    #[test]
    fn lookup_math_command() {
        if let Some(info) = lookup_info("sqrt") {
            assert_eq!(info.arity, Arity::Unary);
            assert_eq!(info.ret, ReturnType::Number);
        }
    }

    #[test]
    fn lookup_string_command() {
        if let Some(info) = lookup_info("toupper") {
            assert_eq!(info.arity, Arity::Unary);
            assert_eq!(info.ret, ReturnType::String);
        }
    }
}
