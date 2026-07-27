// SQF command database — Arma 3 command registry.
//
// Primary source: arma3-wiki crate — tries remote git on startup (6-hour cache),
// falls back to build-time embedded data. Covers ~3,200 Arma 3 commands.
// Static eval subset always overlaid (math/string/array native implementations).

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

/// Database metadata.
#[derive(Debug, Clone)]
pub(crate) struct WikiMeta {
    /// Source of wiki data: "git", "cache", or "embedded"
    pub source: &'static str,
    /// Wiki data version (Arma 3 version the wiki was last synced from)
    pub major: u8,
    pub minor: u8,
    /// Number of commands in wiki
    pub command_count: usize,
}

struct Database {
    commands: HashMap<String, (Arity, bool)>,
    meta: WikiMeta,
}

impl Database {
    fn load() -> Self {
        let mut commands: HashMap<String, (Arity, bool)> = HashMap::new();

        // Static eval commands always available — power our eval implementations
        for &(name, arity, is_arith) in EVAL_COMMANDS {
            commands.insert(name.to_string(), (arity, is_arith));
        }

        // Load arma3-wiki data
        let wiki = std::panic::catch_unwind(|| arma3_wiki::Wiki::load(false)).ok();
        let meta = match &wiki {
            Some(w) => {
                let v = w.version();
                let source = if w.updated() { "git" } else { "cache" };
                let n = w.commands().iter().count();

                // Populate command map
                let n0 = commands.len();
                for (name, cmd) in w.commands().iter() {
                    for syn in cmd.syntax() {
                        let arity = match syn.call() {
                            Call::Nular => Arity::Nular,
                            Call::Unary(_) => Arity::Unary,
                            Call::Binary(_, _) => Arity::Binary,
                        };
                        let is_arith = matches!(syn.ret().typ(), arma3_wiki::model::Value::Number);

                        let entry = commands.entry(name.clone());
                        match entry {
                            std::collections::hash_map::Entry::Occupied(mut o) => {
                                let existing = o.get().0;
                                let better = matches!(
                                    (existing, &arity),
                                    (Arity::Binary, Arity::Unary)
                                        | (Arity::Binary, Arity::Nular)
                                        | (Arity::Unary, Arity::Nular)
                                );
                                if better {
                                    o.insert((arity, is_arith));
                                }
                            }
                            std::collections::hash_map::Entry::Vacant(v) => {
                                v.insert((arity, is_arith));
                            }
                        }
                    }
                }
                let n1 = commands.len();
                eprintln!(
                    "[a3sql] SQF DB: {} commands (+{} from arma3-wiki v{}.{} {})",
                    n1,
                    n1 - n0,
                    v.major(),
                    v.minor(),
                    source,
                );

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

        // Version drift check: warn if wiki data doesn't match configured game version
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

    fn lookup(&self, name: &str) -> Option<Arity> {
        self.commands.get(name).map(|(a, _)| *a)
    }
}

fn global_db() -> &'static Database {
    static DB: OnceLock<Database> = OnceLock::new();
    DB.get_or_init(Database::load)
}

/// Look up a command name (lowercased) and return its arity.
pub(crate) fn lookup(name: &str) -> Option<Arity> {
    global_db().lookup(name)
}

/// Check if a name is a known command.
pub(crate) fn is_command(name: &str) -> bool {
    global_db().lookup(name).is_some()
}

/// Wiki metadata (source, version, command count, max Arma 3 version).
pub(crate) fn wiki_meta() -> &'static WikiMeta {
    &global_db().meta
}

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
}

// ── Static commands (always available, powers eval implementations) ────

static EVAL_COMMANDS: &[(&str, Arity, bool)] = &[
    ("pi", Arity::Nular, true),
    ("true", Arity::Nular, false),
    ("false", Arity::Nular, false),
    ("nil", Arity::Nular, false),
    ("abs", Arity::Unary, true),
    ("acos", Arity::Unary, true),
    ("asin", Arity::Unary, true),
    ("atan", Arity::Unary, true),
    ("ceil", Arity::Unary, true),
    ("cos", Arity::Unary, true),
    ("count", Arity::Unary, true),
    ("deg", Arity::Unary, true),
    ("exp", Arity::Unary, true),
    ("floor", Arity::Unary, true),
    ("hint", Arity::Unary, false),
    ("hintc", Arity::Unary, false),
    ("ln", Arity::Unary, true),
    ("log", Arity::Unary, true),
    ("log10", Arity::Unary, true),
    ("parsenumber", Arity::Unary, true),
    ("parse_number", Arity::Unary, true),
    ("rad", Arity::Unary, true),
    ("round", Arity::Unary, true),
    ("sin", Arity::Unary, true),
    ("sqrt", Arity::Unary, true),
    ("str", Arity::Unary, false),
    ("tan", Arity::Unary, true),
    ("to_string", Arity::Unary, false),
    ("to_lower", Arity::Unary, false),
    ("to_upper", Arity::Unary, false),
    ("tolower", Arity::Unary, false),
    ("toupper", Arity::Unary, false),
    ("type_name", Arity::Unary, false),
    ("typename", Arity::Unary, false),
];
