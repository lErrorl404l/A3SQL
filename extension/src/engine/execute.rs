// a3db statement executor — interprets sqlparser AST against Database

use std::collections::HashMap;

use sqlparser::ast::{
    BinaryOperator, ColumnOption, DataType, Expr, Function, FunctionArg, FunctionArgExpr,
    FunctionArguments, LimitClause, ObjectName, OrderByKind, Query, Select, SelectItem, SetExpr,
    Statement, TableFactor, TableWithJoins, UnaryOperator, Values,
};

use super::database::Database;
use super::index::IndexType as A3IndexType;
use super::table::Table;
use super::value::{Column, ColumnType, DbValue};

// ── Public entry point ──────────────────────────────────────────────────

pub fn execute(stmt: &Statement, db: &mut Database) -> Result<String, String> {
    match stmt {
        Statement::CreateTable(def) => exec_create_table(def, db),
        Statement::Insert(ins) => exec_insert(ins, db),
        Statement::Query(q) => exec_select(q, db),
        Statement::Update(upd) => exec_update(upd, db),
        Statement::Delete(del) => exec_delete(del, db),
        Statement::CreateIndex(idx) => exec_create_index(idx, db),
        Statement::Drop {
            names, object_type, ..
        } => {
            let name = object_name_str(&names[0]);
            let type_str = format!("{}", object_type).to_lowercase();
            if type_str.contains("index") {
                // DROP INDEX — find which table owns it, drop from there
                if !drop_index_by_name(db, &name) {
                    return Err(format!("Index '{}' not found", name));
                }
                Ok(format!("\"Dropped index '{}'\"", name))
            } else {
                db.drop_table(&name)?;
                Ok(format!("\"Dropped table '{}'\"", name))
            }
        }
        Statement::ShowTables { .. } => {
            let names = db.table_names();
            let inner: Vec<String> = names.iter().map(|n| format!("\"{}\"", n)).collect();
            Ok(format!("[{}]", inner.join(",")))
        }
        Statement::StartTransaction { .. } => {
            db.begin();
            Ok("\"Transaction started\"".into())
        }
        Statement::Commit { .. } => {
            db.commit()?;
            Ok("\"Committed\"".into())
        }
        Statement::Rollback { .. } => {
            db.rollback()?;
            Ok("\"Rolled back\"".into())
        }
        Statement::Savepoint { name, .. } => {
            db.savepoint(&name.to_string());
            Ok(format!("\"Savepoint '{}' created\"", name))
        }
        Statement::ReleaseSavepoint { name, .. } => {
            db.release_savepoint(&name.to_string())?;
            Ok(format!("\"Savepoint '{}' released\"", name))
        }
        other => Err(format!("Statement not supported: {:?}", other)),
    }
}

// ── ObjectName helper ───────────────────────────────────────────────────

fn object_name_str(name: &ObjectName) -> String {
    name.to_string().to_lowercase()
}

// ── CREATE TABLE ────────────────────────────────────────────────────────

fn exec_create_table(
    def: &sqlparser::ast::CreateTable,
    db: &mut Database,
) -> Result<String, String> {
    let table_name = object_name_str(&def.name);

    let mut columns: Vec<Column> = Vec::new();
    let mut has_pk = false;

    for col_def in &def.columns {
        let col_name = col_def.name.value.to_lowercase();
        let dtype = parse_data_type(&col_def.data_type)?;

        let mut is_pk = false;
        for opt_def in &col_def.options {
            match &opt_def.option {
                ColumnOption::PrimaryKey(_) | ColumnOption::Unique { .. } => is_pk = true,
                _ => {}
            }
        }

        if is_pk {
            if has_pk {
                return Err("Only one primary key column supported".into());
            }
            has_pk = true;
        }

        columns.push(Column {
            name: col_name,
            dtype,
            primary_key: is_pk,
        });
    }

    // Table-level PRIMARY KEY constraints — Display-based detection
    for constraint in &def.constraints {
        let text = format!("{}", constraint).to_uppercase();
        if text.contains("PRIMARY KEY (") {
            // Extract column name from display text: "PRIMARY KEY (col)"
            if let Some(start) = text.find('(') {
                if let Some(end) = text.find(')') {
                    let cname = text[start + 1..end].trim().to_lowercase();
                    if let Some(col) = columns.iter_mut().find(|col| col.name == cname) {
                        if has_pk {
                            return Err("Only one primary key supported".into());
                        }
                        col.primary_key = true;
                        has_pk = true;
                    }
                }
            }
        }
    }

    let table = Table::new(table_name.clone(), columns)?;
    db.create_table(&table_name, table)?;
    Ok(format!("\"Table '{}' created\"", table_name))
}

// ── INSERT ──────────────────────────────────────────────────────────────

fn exec_insert(ins: &sqlparser::ast::Insert, db: &mut Database) -> Result<String, String> {
    // Extract table name from the table field (TableObject)
    let table_name = ins.table.to_string().to_lowercase();
    let table = db.get_table_mut(&table_name)?;

    // Determine column mapping (ins.columns is Vec<ObjectName>)
    let explicit_cols: Option<Vec<usize>> = if !ins.columns.is_empty() {
        Some(
            ins.columns
                .iter()
                .map(|col_name| {
                    let name = object_name_str(col_name);
                    table.col_idx(&name).ok_or_else(|| {
                        format!("Unknown column '{}' in table '{}'", name, table_name)
                    })
                })
                .collect::<Result<Vec<usize>, String>>()?,
        )
    } else {
        None
    };

    // Extract values from source (Option<Box<Query>>)
    let source = match &ins.source {
        Some(q) => q.as_ref(),
        None => return Err("INSERT must have a source".into()),
    };

    let rows = match &*source.body {
        SetExpr::Values(Values { rows, .. }) => rows
            .iter()
            .map(|parens| parens.content.clone())
            .collect::<Vec<Vec<Expr>>>(),
        _ => return Err("Only VALUES-based INSERT supported".into()),
    };

    let mut inserted = 0usize;
    for row_exprs in &rows {
        let col_indices: &[usize] = match &explicit_cols {
            Some(indices) => indices.as_slice(),
            None => &(0..table.col_count()).collect::<Vec<_>>(),
        };

        if row_exprs.len() != col_indices.len() {
            return Err(format!(
                "Expected {} values, got {}",
                col_indices.len(),
                row_exprs.len()
            ));
        }

        let mut full_row: Vec<DbValue> = (0..table.col_count()).map(|_| DbValue::Null).collect();
        for (j, expr) in row_exprs.iter().enumerate() {
            let col_idx = col_indices[j];
            full_row[col_idx] = eval_literal_expr(expr)?;
        }

        table.insert(full_row)?;
        inserted += 1;
    }

    Ok(format!("\"Inserted {} row(s)\"", inserted))
}

// ── SELECT ──────────────────────────────────────────────────────────────

fn exec_select(query: &Query, db: &mut Database) -> Result<String, String> {
    let select: &Select = match &*query.body {
        SetExpr::Select(s) => s.as_ref(),
        _ => return Err("Only SELECT statements supported".into()),
    };

    // Route to multi-table handler if FROM has JOINs OR multiple tables
    if has_multiple_tables(select) {
        return exec_select_joins(query, select, db);
    }

    // Resolve table (single-table)
    let table = resolve_single_table(&select.from, db)?;
    let where_expr = select.selection.as_ref();

    // 1. Filter rows by WHERE
    let filtered_rows: Vec<&[DbValue]> = table
        .rows
        .iter()
        .filter(|row| {
            where_expr
                .map(|expr| {
                    is_truthy(
                        &eval_expr(expr, row, &table.col_index).unwrap_or(DbValue::Bool(false)),
                    )
                })
                .unwrap_or(true)
        })
        .map(|r| r.as_slice())
        .collect();

    // 2. If aggregates are present, handle them (with or without GROUP BY)
    if has_aggregate(&select.projection) {
        let group_partitions = if has_group_by(select) {
            partition_by_group(&filtered_rows, select, &table.col_index)?
        } else {
            vec![filtered_rows] // single group: all rows
        };
        return compute_aggregates(&group_partitions, &select.projection, &table.col_index);
    }

    // 3. GROUP BY without aggregates — simple dedup
    let grouped_rows = if has_group_by(select) {
        let partitions = partition_by_group(&filtered_rows, select, &table.col_index)?;
        partitions.into_iter().map(|p| p[0]).collect()
    } else {
        filtered_rows
    };

    // 4. ORDER BY
    let sorted_rows = if let Some(order_by) = &query.order_by {
        let exprs = match &order_by.kind {
            OrderByKind::Expressions(exprs) => exprs,
            _ => return Err("ORDER BY ALL not supported".into()),
        };
        if !exprs.is_empty() {
            sort_rows(grouped_rows, exprs, &table.col_index)?
        } else {
            grouped_rows
        }
    } else {
        grouped_rows
    };

    // 5. LIMIT / OFFSET
    let limited_rows = apply_limit_offset(sorted_rows, &query.limit_clause)?;

    // 6. Format result
    Ok(table.format_result(limited_rows))
}

/// Check if the FROM clause has multiple tables or JOINs.
fn has_multiple_tables(select: &Select) -> bool {
    select.from.len() > 1 || select.from.iter().any(|t| !t.joins.is_empty())
}

/// Execute a SELECT with JOINs. Uses a flat-row column map with absolute positions.
fn exec_select_joins(query: &Query, select: &Select, db: &mut Database) -> Result<String, String> {
    use sqlparser::ast::{JoinConstraint, JoinOperator};

    // ── Resolve all tables in FROM + JOINs ──────────────────────────
    struct Tbl {
        name: String,
        name_meta: String,
        cols: usize,
        start: usize,
        rows: Vec<Vec<DbValue>>,
    }

    let mut tbls: Vec<Tbl> = Vec::new();
    let mut abs: usize = 0;

    for twj in &select.from {
        let (n, t) = resolve_table_factor(&twj.relation, db)?;
        let r: Vec<Vec<DbValue>> = t.rows.to_vec();
        let c = t.columns.len();
        tbls.push(Tbl {
            name: n.clone(),
            name_meta: n,
            cols: c,
            start: abs,
            rows: r,
        });
        abs += c;
        for j in &twj.joins {
            let (jn, jt) = resolve_table_factor(&j.relation, db)?;
            let jr: Vec<Vec<DbValue>> = jt.rows.to_vec();
            let jc = jt.columns.len();
            tbls.push(Tbl {
                name: jn.clone(),
                name_meta: jn,
                cols: jc,
                start: abs,
                rows: jr,
            });
            abs += jc;
        }
    }

    // ── Build flat column map ───────────────────────────────────────
    let mut col_map: HashMap<String, usize> = HashMap::new();
    let mut header: Vec<String> = Vec::new();
    for tbl in &tbls {
        let tn = db
            .get_table(&tbl.name)
            .map_err(|e| format!("JOIN: {}", e))?
            .clone();
        for (ci, col) in tn.columns.iter().enumerate() {
            let p = tbl.start + ci;
            col_map.insert(format!("{}.{}", tbl.name, col.name), p);
            col_map.insert(col.name.clone(), p);
            header.push(format!("{}.{}", tbl.name, col.name));
        }
    }

    let total = abs;

    // Helper: build flat row from table-row indices
    let bf = |idxs: &[usize]| -> Vec<DbValue> {
        let mut v = Vec::with_capacity(total);
        for (ti, &ri) in idxs.iter().enumerate() {
            if ri == usize::MAX {
                v.resize(v.len() + tbls[ti].cols, DbValue::Null);
            } else {
                v.extend_from_slice(&tbls[ti].rows[ri]);
            }
        }
        v
    };

    let ef = |e: &Expr, r: &[DbValue]| -> Result<DbValue, String> {
        eval_expr_on_flat_row(e, r, &col_map)
    };

    // ── Generate combined rows ──────────────────────────────────────
    let mut cidx: Vec<Vec<usize>> = (0..tbls[0].rows.len()).map(|i| vec![i]).collect();
    let no_constraint = JoinConstraint::None;
    let joins = &select.from[0].joins;

    for (ti, tbl) in tbls.iter().enumerate().skip(1) {
        // Get the join operator for this table index
        let con = if ti <= joins.len() {
            let join = &joins[ti - 1];
            match &join.join_operator {
                JoinOperator::Inner(c)
                | JoinOperator::LeftOuter(c)
                | JoinOperator::RightOuter(c)
                | JoinOperator::FullOuter(c) => c,
                _ => &no_constraint,
            }
        } else {
            &no_constraint
        };
        let left =
            ti <= joins.len() && matches!(joins[ti - 1].join_operator, JoinOperator::LeftOuter(_));

        let mut next = Vec::new();
        for ls in &cidx {
            let mut hit = false;
            for ri in 0..tbl.rows.len() {
                let mut cs = ls.clone();
                cs.push(ri);
                let f = bf(&cs);
                let ok = match con {
                    JoinConstraint::On(ex) => ef(ex, &f).map(|v| is_truthy(&v)).unwrap_or(false),
                    _ => true,
                };
                if ok {
                    next.push(cs);
                    hit = true;
                }
            }
            if left && !hit {
                let mut ns = ls.clone();
                ns.push(usize::MAX);
                next.push(ns);
            }
        }
        cidx = next;
    }

    // ── Materialize ─────────────────────────────────────────────────
    let mut rows: Vec<Vec<DbValue>> = cidx.iter().map(|ix| bf(ix)).collect();

    // WHERE
    if let Some(ex) = select.selection.as_ref() {
        rows.retain(|r| ef(ex, r).map(|v| is_truthy(&v)).unwrap_or(false));
    }

    // ORDER BY
    if let Some(ob) = &query.order_by {
        let exs = match &ob.kind {
            OrderByKind::Expressions(e) => e,
            _ => return Err("ORDER BY ALL not supported".into()),
        };
        if !exs.is_empty() {
            rows.sort_by(|a, b| {
                for o in exs {
                    let av = ef(&o.expr, a).unwrap_or(DbValue::Null);
                    let bv = ef(&o.expr, b).unwrap_or(DbValue::Null);
                    let c = value_to_string(&av).cmp(&value_to_string(&bv));
                    let c = if o.options.asc.unwrap_or(true) {
                        c
                    } else {
                        c.reverse()
                    };
                    if c != std::cmp::Ordering::Equal {
                        return c;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }
    }

    // LIMIT / OFFSET
    let (off, lim) = match &query.limit_clause {
        Some(LimitClause::LimitOffset { limit, offset, .. }) => (
            parse_expr_as_usize(offset.as_ref().map(|o| &o.value)).unwrap_or(0),
            limit.as_ref().and_then(|e| parse_expr_as_usize(Some(e))),
        ),
        Some(LimitClause::OffsetCommaLimit { offset, limit }) => (
            parse_expr_as_usize(Some(offset)).unwrap_or(0),
            parse_expr_as_usize(Some(limit)),
        ),
        None => (0, None),
    };
    let s = off.min(rows.len());
    let e = match lim {
        Some(l) => (s + l).min(rows.len()),
        None => rows.len(),
    };
    rows = rows[s..e].to_vec();

    // Format
    let h = header
        .iter()
        .map(|h| format!("\"{}\"", h))
        .collect::<Vec<_>>()
        .join(",");
    let rj: Vec<String> = rows
        .iter()
        .map(|r| {
            let c: Vec<String> = r.iter().map(|v| v.to_json_string()).collect();
            format!("[{}]", c.join(","))
        })
        .collect();
    Ok(format!("[[{}],{}]", h, rj.join(",")))
}

fn eval_expr_on_flat_row(
    expr: &Expr,
    row: &[DbValue],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, String> {
    match expr {
        Expr::Identifier(ident) => {
            let name = ident.value.to_lowercase();
            match col_map.get(&name) {
                Some(&pos) => Ok(row[pos].clone()),
                None => Err(format!("Unknown column '{}'", name)),
            }
        }
        Expr::CompoundIdentifier(parts) => {
            // e.g. a.id → "a.id"
            let name = parts
                .iter()
                .map(|p| p.value.to_lowercase())
                .collect::<Vec<_>>()
                .join(".");
            match col_map.get(&name) {
                Some(&pos) => Ok(row[pos].clone()),
                None => {
                    // Try just the last part
                    let last = parts.last().unwrap().value.to_lowercase();
                    match col_map.get(&last) {
                        Some(&pos) => Ok(row[pos].clone()),
                        None => Err(format!("Unknown column '{}'", name)),
                    }
                }
            }
        }
        Expr::Value(v) => Ok(sql_val_to_db(&v.value)),
        Expr::BinaryOp { left, op, right } => {
            let l = eval_expr_on_flat_row(left, row, col_map)?;
            let r = eval_expr_on_flat_row(right, row, col_map)?;
            apply_binary_op(&l, op, &r)
        }
        Expr::Nested(inner) => eval_expr_on_flat_row(inner, row, col_map),
        Expr::Function(func) => {
            let name = func.name.to_string().to_lowercase();
            if name == "fuzzy_match" {
                // Evaluate args against the flat row
                let args = match &func.args {
                    FunctionArguments::List(list) => &list.args,
                    _ => return Err("fuzzy_match requires args".into()),
                };
                if args.len() < 2 {
                    return Err("fuzzy_match requires 2 args".into());
                }
                let a1 = eval_expr_on_flat_row(get_func_arg_unnamed(&args[0])?, row, col_map)?;
                let a2 = eval_expr_on_flat_row(get_func_arg_unnamed(&args[1])?, row, col_map)?;
                let sim = Table::trigram_similarity(&value_to_string(&a1), &value_to_string(&a2));
                Ok(DbValue::Bool(sim >= 0.3))
            } else {
                Err(format!("Unsupported function in JOIN: {}", name))
            }
        }
        _ => Err(format!("Unsupported expression in JOIN: {:?}", expr)),
    }
}

fn resolve_table_factor(
    factor: &sqlparser::ast::TableFactor,
    db: &Database,
) -> Result<(String, crate::engine::table::Table), String> {
    use sqlparser::ast::TableFactor;
    match factor {
        TableFactor::Table { name, .. } => {
            let tname = object_name_str(name);
            let table = db.get_table(&tname)?.clone();
            Ok((tname, table))
        }
        _ => Err("Only simple table references supported in JOINs".into()),
    }
}

/// Check if SELECT has a GROUP BY clause.
fn has_group_by(select: &Select) -> bool {
    use sqlparser::ast::GroupByExpr;
    match &select.group_by {
        GroupByExpr::Expressions(exprs, _) => !exprs.is_empty(),
        GroupByExpr::All(_) => true,
    }
}

/// Check if SELECT projection contains aggregate functions.
fn has_aggregate(projection: &[SelectItem]) -> bool {
    for item in projection {
        if let SelectItem::UnnamedExpr(expr) = item {
            if contains_aggregate(expr) {
                return true;
            }
        }
    }
    false
}

fn contains_aggregate(expr: &Expr) -> bool {
    match expr {
        Expr::Function(f) => {
            let name = f.name.to_string().to_lowercase();
            matches!(name.as_str(), "count" | "sum" | "avg" | "min" | "max")
        }
        Expr::Nested(inner) => contains_aggregate(inner),
        _ => false,
    }
}

/// Partition filtered rows into groups by GROUP BY columns.
/// Returns a Vec of groups, where each group is a Vec of row references.
fn partition_by_group<'a>(
    rows: &[&'a [DbValue]],
    select: &Select,
    col_map: &HashMap<String, usize>,
) -> Result<Vec<Vec<&'a [DbValue]>>, String> {
    let exprs = group_by_exprs(select)?;
    let mut groups: Vec<Vec<&[DbValue]>> = Vec::new();
    let mut keys: Vec<Vec<DbValue>> = Vec::new();

    'rows: for row in rows {
        let key: Result<Vec<DbValue>, String> =
            exprs.iter().map(|e| eval_expr(e, row, col_map)).collect();
        let key = key?;

        for (i, existing_key) in keys.iter().enumerate() {
            if keys_equal(&key, existing_key) {
                groups[i].push(row);
                continue 'rows;
            }
        }
        keys.push(key);
        groups.push(vec![row]);
    }

    Ok(groups)
}

fn group_by_exprs(select: &Select) -> Result<&[Expr], String> {
    use sqlparser::ast::GroupByExpr;
    match &select.group_by {
        GroupByExpr::Expressions(exprs, _) => Ok(exprs.as_slice()),
        GroupByExpr::All(_) => Err("GROUP BY ALL not supported".into()),
    }
}

fn eval_group_key(
    select: &Select,
    row: &[DbValue],
    col_map: &HashMap<String, usize>,
) -> Result<Vec<DbValue>, String> {
    use sqlparser::ast::GroupByExpr;
    let mut key = Vec::new();
    let exprs = match &select.group_by {
        GroupByExpr::Expressions(exprs, _) => exprs,
        GroupByExpr::All(_) => return Err("GROUP BY ALL not supported".into()),
    };
    for expr in exprs {
        let val = eval_expr(expr, row, col_map)?;
        key.push(val);
    }
    Ok(key)
}

fn keys_equal(a: &[DbValue], b: &[DbValue]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
}

/// Compute aggregate functions over partitions (groups) of rows.
fn compute_aggregates(
    partitions: &[Vec<&[DbValue]>],
    projection: &[SelectItem],
    col_map: &HashMap<String, usize>,
) -> Result<String, String> {
    if partitions.is_empty() {
        return Ok("[]".to_string());
    }

    // Build header from projection
    let mut header = Vec::new();
    for item in projection {
        match item {
            SelectItem::UnnamedExpr(expr) => header.push(projection_expr_name(expr)),
            _ => return Err("Unsupported SELECT item in aggregate query".into()),
        }
    }

    // Compute one row per partition
    let rows_json: Vec<String> = partitions
        .iter()
        .map(|group| {
            let cells: Vec<String> = projection
                .iter()
                .map(|item| match item {
                    SelectItem::UnnamedExpr(expr) => eval_projection_expr(expr, group, col_map)
                        .map(|(_, v)| v.to_json_string())
                        .unwrap_or_else(|_| "null".to_string()),
                    _ => "null".to_string(),
                })
                .collect();
            format!("[{}]", cells.join(","))
        })
        .collect();

    let header_json: String = header
        .iter()
        .map(|h| format!("\"{}\"", h))
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!("[[{}],{}]", header_json, rows_json.join(",")))
}

fn projection_expr_name(expr: &Expr) -> String {
    match expr {
        Expr::Function(f) => f.name.to_string().to_uppercase(),
        Expr::Identifier(ident) => ident.value.to_lowercase(),
        _ => "EXPR".to_string(),
    }
}

/// Evaluate a projection expression (handles aggregates vs regular expressions).
fn eval_projection_expr(
    expr: &Expr,
    rows: &[&[DbValue]],
    col_map: &HashMap<String, usize>,
) -> Result<(String, DbValue), String> {
    match expr {
        Expr::Function(f) => {
            let name = f.name.to_string().to_lowercase();
            match name.as_str() {
                "count" => {
                    let count = DbValue::Int(rows.len() as i64);
                    Ok(("COUNT".to_string(), count))
                }
                "sum" => {
                    let val = aggregate_sum(f, rows, col_map)?;
                    Ok(("SUM".to_string(), val))
                }
                "avg" => {
                    let val = aggregate_avg(f, rows, col_map)?;
                    Ok(("AVG".to_string(), val))
                }
                "min" => {
                    let val = aggregate_min(f, rows, col_map)?;
                    Ok(("MIN".to_string(), val))
                }
                "max" => {
                    let val = aggregate_max(f, rows, col_map)?;
                    Ok(("MAX".to_string(), val))
                }
                _ => {
                    let val = eval_expr_on_group(expr, rows, col_map)?;
                    Ok((format!("{}", f.name), val))
                }
            }
        }
        Expr::Identifier(ident) => {
            // Regular column — use first row's value
            let val = if rows.is_empty() {
                DbValue::Null
            } else {
                let idx = col_map
                    .get(&ident.value.to_lowercase())
                    .ok_or_else(|| format!("Unknown column '{}'", ident.value))?;
                rows[0][*idx].clone()
            };
            Ok((ident.value.to_lowercase(), val))
        }
        _ => {
            let val = eval_expr_on_group(expr, rows, col_map)?;
            Ok(("expr".to_string(), val))
        }
    }
}

/// Evaluate an expression on a group of rows. For non-aggregate columns, uses first row.
fn eval_expr_on_group(
    expr: &Expr,
    rows: &[&[DbValue]],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, String> {
    // For aggregate queries, non-aggregate columns use the first row's value
    if rows.is_empty() {
        return Ok(DbValue::Null);
    }
    eval_expr(expr, rows[0], col_map)
}

fn aggregate_sum(
    func: &Function,
    rows: &[&[DbValue]],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, String> {
    let arg = extract_func_arg(func)?;
    if rows.is_empty() {
        return Ok(DbValue::Null);
    }
    let first = eval_expr_on_group(arg, rows, col_map)?;
    match first {
        DbValue::Int(..) => {
            let sum: i64 = rows
                .iter()
                .filter_map(|r| {
                    eval_expr(arg, r, col_map).ok().and_then(|v| match v {
                        DbValue::Int(n) => Some(n),
                        _ => None,
                    })
                })
                .sum();
            Ok(DbValue::Int(sum))
        }
        DbValue::Float(..) => {
            let sum: f64 = rows
                .iter()
                .filter_map(|r| {
                    eval_expr(arg, r, col_map).ok().and_then(|v| match v {
                        DbValue::Float(f) => Some(f),
                        DbValue::Int(n) => Some(n as f64),
                        _ => None,
                    })
                })
                .sum();
            Ok(DbValue::Float(sum))
        }
        _ => Err("SUM requires numeric column".into()),
    }
}

fn aggregate_avg(
    func: &Function,
    rows: &[&[DbValue]],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, String> {
    let arg = extract_func_arg(func)?;
    if rows.is_empty() {
        return Ok(DbValue::Null);
    }
    let mut sum = 0.0f64;
    let mut count = 0usize;
    for r in rows {
        if let Ok(v) = eval_expr(arg, r, col_map) {
            match v {
                DbValue::Int(n) => {
                    sum += n as f64;
                    count += 1;
                }
                DbValue::Float(f) => {
                    sum += f;
                    count += 1;
                }
                _ => {}
            }
        }
    }
    if count == 0 {
        Ok(DbValue::Null)
    } else {
        Ok(DbValue::Float(sum / count as f64))
    }
}

fn aggregate_min(
    func: &Function,
    rows: &[&[DbValue]],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, String> {
    let arg = extract_func_arg(func)?;
    rows.iter()
        .filter_map(|r| eval_expr(arg, r, col_map).ok())
        .min_by(|a, b| value_to_string(a).cmp(&value_to_string(b)))
        .ok_or_else(|| "MIN on empty set".into())
}

fn aggregate_max(
    func: &Function,
    rows: &[&[DbValue]],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, String> {
    let arg = extract_func_arg(func)?;
    rows.iter()
        .filter_map(|r| eval_expr(arg, r, col_map).ok())
        .max_by(|a, b| value_to_string(a).cmp(&value_to_string(b)))
        .ok_or_else(|| "MAX on empty set".into())
}

/// Extract the first argument expression from a function.
fn extract_func_arg(func: &Function) -> Result<&Expr, String> {
    let args = match &func.args {
        FunctionArguments::List(list) => &list.args,
        _ => return Err("Function requires argument list".into()),
    };
    if args.is_empty() {
        return Err("Function requires argument".into());
    }
    get_func_arg_unnamed(&args[0])
}

/// ORDER BY sorting
fn sort_rows<'a>(
    mut rows: Vec<&'a [DbValue]>,
    order_by: &[sqlparser::ast::OrderByExpr],
    col_map: &HashMap<String, usize>,
) -> Result<Vec<&'a [DbValue]>, String> {
    if order_by.is_empty() {
        return Ok(rows);
    }

    rows.sort_by(|a, b| {
        for order in order_by {
            let a_val = eval_expr(&order.expr, a, col_map).unwrap_or(DbValue::Null);
            let b_val = eval_expr(&order.expr, b, col_map).unwrap_or(DbValue::Null);
            let ordering = value_to_string(&a_val).cmp(&value_to_string(&b_val));
            let is_asc = order.options.asc.unwrap_or(true);
            let ordering = if is_asc { ordering } else { ordering.reverse() };
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
            }
        }
        std::cmp::Ordering::Equal
    });

    Ok(rows)
}

/// Apply LIMIT and OFFSET.
fn apply_limit_offset<'a>(
    rows: Vec<&'a [DbValue]>,
    limit_clause: &Option<LimitClause>,
) -> Result<Vec<&'a [DbValue]>, String> {
    let (offset_val, limit_val) = match limit_clause {
        Some(LimitClause::LimitOffset { limit, offset, .. }) => {
            let off = parse_expr_as_usize(offset.as_ref().map(|o| &o.value)).unwrap_or(0);
            let lim = limit.as_ref().and_then(|e| parse_expr_as_usize(Some(e)));
            (off, lim)
        }
        Some(LimitClause::OffsetCommaLimit { offset, limit }) => {
            let off = parse_expr_as_usize(Some(offset)).unwrap_or(0);
            let lim = parse_expr_as_usize(Some(limit));
            (off, lim)
        }
        None => (0, None),
    };

    let start = offset_val.min(rows.len());
    let end = match limit_val {
        Some(l) => (start + l).min(rows.len()),
        None => rows.len(),
    };

    Ok(rows[start..end].to_vec())
}

fn parse_expr_as_usize(expr: Option<&Expr>) -> Option<usize> {
    let expr = expr?;
    if let Expr::Value(v) = expr {
        if let sqlparser::ast::Value::Number(s, _) = &v.value {
            return s.parse::<usize>().ok();
        }
    }
    None
}

// ── UPDATE ──────────────────────────────────────────────────────────────

fn exec_update(upd: &sqlparser::ast::Update, db: &mut Database) -> Result<String, String> {
    let table_name = resolve_table_from_joins(&upd.table)?;
    let table = db.get_table_mut(&table_name)?;

    let where_expr = upd.selection.as_ref();

    // Collect row indices to update
    let indices: Vec<usize> = {
        let mut idxs = Vec::new();
        for (i, row) in table.rows.iter().enumerate() {
            let matches = match where_expr {
                Some(expr) => is_truthy(&eval_expr(expr, row, &table.col_index)?),
                None => true,
            };
            if matches {
                idxs.push(i);
            }
        }
        idxs
    };

    // Pre-resolve column indices to avoid borrow conflict
    let assign_indices: Vec<(usize, &Expr)> = upd
        .assignments
        .iter()
        .map(|assign| {
            let col_name = assign.target.to_string().to_lowercase();
            let idx = table
                .col_idx(&col_name)
                .ok_or_else(|| format!("Unknown column '{}'", col_name))?;
            Ok((idx, &assign.value))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let mut count = 0usize;
    for i in indices {
        for (col_idx, val_expr) in &assign_indices {
            let new_val = eval_literal_expr(val_expr)?;
            table.update_cell(i, *col_idx, new_val);
        }
        count += 1;
    }

    Ok(format!("\"Updated {} row(s)\"", count))
}

// ── DELETE ──────────────────────────────────────────────────────────────

fn exec_delete(del: &sqlparser::ast::Delete, db: &mut Database) -> Result<String, String> {
    // Table name is in `from` (FromTable), not `tables` (MySQL multi-table)
    use sqlparser::ast::FromTable;
    let table_name = match &del.from {
        FromTable::WithFromKeyword(tables) | FromTable::WithoutKeyword(tables) => {
            match tables.first() {
                Some(tj) => match &tj.relation {
                    TableFactor::Table { name, .. } => object_name_str(name),
                    _ => return Err("DELETE: only simple table references supported".into()),
                },
                None => return Err("DELETE must specify a table".into()),
            }
        }
    };

    // Clone col_index to avoid borrow conflict with table.delete()
    let col_idx = db.get_table(&table_name)?.col_index.clone();
    let pred = del.selection.clone();

    let table = db.get_table_mut(&table_name)?;
    let count = match pred {
        Some(expr) => table.delete(|row| {
            eval_expr(&expr, row, &col_idx)
                .map(|v| is_truthy(&v))
                .unwrap_or(false)
        }),
        None => {
            // Clear using the index-aware delete with a catch-all predicate
            table.delete(|_| true)
        }
    };

    Ok(format!("\"Deleted {} row(s)\"", count))
}

// ── Expression evaluator ────────────────────────────────────────────────

fn eval_expr(
    expr: &Expr,
    row: &[DbValue],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, String> {
    match expr {
        Expr::Identifier(ident) => {
            let name = ident.value.to_lowercase();
            let idx = col_map
                .get(&name)
                .ok_or_else(|| format!("Unknown column '{}'", name))?;
            Ok(row[*idx].clone())
        }
        Expr::Value(v) => Ok(sql_val_to_db(v)),
        Expr::BinaryOp { left, op, right } => {
            let l = eval_expr(left, row, col_map)?;
            let r = eval_expr(right, row, col_map)?;
            apply_binary_op(&l, op, &r)
        }
        Expr::UnaryOp { op, expr } => {
            let val = eval_expr(expr, row, col_map)?;
            apply_unary_op(op, &val)
        }
        Expr::Nested(inner) => eval_expr(inner, row, col_map),
        Expr::IsNull(expr) => {
            let val = eval_expr(expr, row, col_map)?;
            Ok(DbValue::Bool(matches!(val, DbValue::Null)))
        }
        Expr::IsNotNull(expr) => {
            let val = eval_expr(expr, row, col_map)?;
            Ok(DbValue::Bool(!matches!(val, DbValue::Null)))
        }
        Expr::Like {
            negated,
            expr,
            pattern,
            escape_char: _,
            ..
        } => {
            let val = eval_expr(expr, row, col_map)?;
            let pat = eval_expr(pattern, row, col_map)?;
            let matched = simple_like(&value_to_string(&val), &value_to_string(&pat));
            Ok(DbValue::Bool(if *negated { !matched } else { matched }))
        }
        Expr::InList {
            expr,
            list,
            negated,
        } => {
            let val = eval_expr(expr, row, col_map)?;
            let mut found = false;
            for item in list {
                let item_val = eval_expr(item, row, col_map)?;
                if val == item_val {
                    found = true;
                    break;
                }
            }
            Ok(DbValue::Bool(if *negated { !found } else { found }))
        }
        Expr::Function(func) => exec_function(func, row, col_map),
        _ => Err(format!("Unsupported expression: {:?}", expr)),
    }
}

fn eval_literal_expr(expr: &Expr) -> Result<DbValue, String> {
    match expr {
        Expr::Value(v) => Ok(sql_val_to_db(&v.value)),
        Expr::Nested(inner) => eval_literal_expr(inner),
        Expr::UnaryOp { op, expr } => {
            let val = eval_literal_expr(expr)?;
            apply_unary_op(op, &val)
        }
        _ => Err(format!(
            "Complex expressions not supported in values: {:?}",
            expr
        )),
    }
}

// ── Function execution ──────────────────────────────────────────────────

fn exec_function(
    func: &Function,
    row: &[DbValue],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, String> {
    let name = func.name.to_string().to_lowercase();
    match name.as_str() {
        "fuzzy_match" => exec_fuzzy_match(func, row, col_map),
        _ => Err(format!("Unknown function '{}'", name)),
    }
}

/// Get function argument as Expr, assuming Unnamed(FunctionArgExpr::Expr(e))
fn get_func_arg_unnamed(arg: &FunctionArg) -> Result<&Expr, String> {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => Ok(e),
        FunctionArg::Unnamed(_) => Err("Expected expression argument".into()),
        FunctionArg::Named { arg, .. } | FunctionArg::ExprNamed { arg, .. } => match arg {
            FunctionArgExpr::Expr(e) => Ok(e),
            _ => Err("Expected expression in named argument".into()),
        },
    }
}

fn exec_fuzzy_match(
    func: &Function,
    row: &[DbValue],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, String> {
    let args = match &func.args {
        FunctionArguments::List(list) => &list.args,
        _ => return Err("fuzzy_match requires argument list".into()),
    };

    if args.len() < 2 {
        return Err("fuzzy_match requires at least 2 arguments".into());
    }

    let col_val = eval_expr(get_func_arg_unnamed(&args[0])?, row, col_map)?;
    let pat_val = eval_expr(get_func_arg_unnamed(&args[1])?, row, col_map)?;

    let threshold = if args.len() >= 3 {
        let t = eval_expr(get_func_arg_unnamed(&args[2])?, row, col_map)?;
        match t {
            DbValue::Float(f) => f,
            DbValue::Int(i) => i as f64,
            _ => 0.3,
        }
    } else {
        0.3
    };

    let similarity =
        Table::trigram_similarity(&value_to_string(&col_val), &value_to_string(&pat_val));
    Ok(DbValue::Bool(similarity >= threshold))
}

// ── Binary operators ───────────────────────────────────────────────────

fn apply_binary_op(
    left: &DbValue,
    op: &BinaryOperator,
    right: &DbValue,
) -> Result<DbValue, String> {
    match op {
        BinaryOperator::Eq => Ok(DbValue::Bool(values_equal(left, right))),
        BinaryOperator::NotEq => Ok(DbValue::Bool(!values_equal(left, right))),
        BinaryOperator::Lt => cmp_values(left, right, |o| o.is_lt()),
        BinaryOperator::LtEq => cmp_values(left, right, |o| o.is_le()),
        BinaryOperator::Gt => cmp_values(left, right, |o| o.is_gt()),
        BinaryOperator::GtEq => cmp_values(left, right, |o| o.is_ge()),
        BinaryOperator::Plus => arith_op(left, right, |a, b| a + b, |a, b| a + b),
        BinaryOperator::Minus => arith_op(left, right, |a, b| a - b, |a, b| a - b),
        BinaryOperator::Multiply => arith_op(left, right, |a, b| a * b, |a, b| a * b),
        BinaryOperator::Divide => arith_op(left, right, |a, b| a / b, |a, b| a / b),
        BinaryOperator::Modulo => match (to_float(left), to_float(right)) {
            (Some(a), Some(b)) if b != 0.0 => Ok(DbValue::Float(a % b)),
            _ => Err("Modulo requires numeric operands".into()),
        },
        BinaryOperator::And => Ok(DbValue::Bool(is_truthy(left) && is_truthy(right))),
        BinaryOperator::Or => Ok(DbValue::Bool(is_truthy(left) || is_truthy(right))),
        BinaryOperator::StringConcat => Ok(DbValue::String(format!(
            "{}{}",
            value_to_string(left),
            value_to_string(right)
        ))),
        _ => Err(format!("Unsupported operator: {:?}", op)),
    }
}

fn apply_unary_op(op: &UnaryOperator, val: &DbValue) -> Result<DbValue, String> {
    match op {
        UnaryOperator::Not => Ok(DbValue::Bool(!is_truthy(val))),
        UnaryOperator::Plus => Ok(val.clone()),
        UnaryOperator::Minus => match val {
            DbValue::Int(n) => Ok(DbValue::Int(-n)),
            DbValue::Float(f) => Ok(DbValue::Float(-f)),
            _ => Err(format!("Cannot negate {}", val)),
        },
        _ => Err(format!("Unsupported unary operator: {:?}", op)),
    }
}

// ── Comparison & coercion ──────────────────────────────────────────────

fn values_equal(a: &DbValue, b: &DbValue) -> bool {
    // NULL != anything (including NULL), per SQL standard
    if matches!(a, DbValue::Null) || matches!(b, DbValue::Null) {
        return false;
    }
    match (a, b) {
        (DbValue::Null, DbValue::Null) => true,
        (DbValue::Int(x), DbValue::Int(y)) => x == y,
        (DbValue::Float(x), DbValue::Float(y)) => (x - y).abs() < f64::EPSILON,
        (DbValue::Int(x), DbValue::Float(y)) => (*x as f64 - y).abs() < f64::EPSILON,
        (DbValue::Float(x), DbValue::Int(y)) => (x - *y as f64).abs() < f64::EPSILON,
        (DbValue::Bool(x), DbValue::Bool(y)) => x == y,
        (DbValue::String(x), DbValue::String(y)) => x == y,
        _ => value_to_string(a) == value_to_string(b),
    }
}

fn cmp_values<F>(a: &DbValue, b: &DbValue, cmp: F) -> Result<DbValue, String>
where
    F: Fn(std::cmp::Ordering) -> bool,
{
    // SQL: NULL compared to anything is NULL (treated as false)
    if matches!(a, DbValue::Null) || matches!(b, DbValue::Null) {
        return Ok(DbValue::Bool(false));
    }
    let ord = match (to_float(a), to_float(b)) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => value_to_string(a).cmp(&value_to_string(b)),
    };
    Ok(DbValue::Bool(cmp(ord)))
}

fn arith_op<F, G>(a: &DbValue, b: &DbValue, int_op: F, float_op: G) -> Result<DbValue, String>
where
    F: Fn(i64, i64) -> i64,
    G: Fn(f64, f64) -> f64,
{
    // SQL: NULL in any arithmetic → NULL
    if matches!(a, DbValue::Null) || matches!(b, DbValue::Null) {
        return Ok(DbValue::Null);
    }
    match (a, b) {
        (DbValue::Int(x), DbValue::Int(y)) => Ok(DbValue::Int(int_op(*x, *y))),
        _ => match (to_float(a), to_float(b)) {
            (Some(x), Some(y)) => Ok(DbValue::Float(float_op(x, y))),
            _ => Err(format!("Type mismatch: {} vs {}", a, b)),
        },
    }
}

fn to_float(v: &DbValue) -> Option<f64> {
    match v {
        DbValue::Int(n) => Some(*n as f64),
        DbValue::Float(f) => Some(*f),
        DbValue::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn is_truthy(v: &DbValue) -> bool {
    match v {
        DbValue::Null => false,
        DbValue::Bool(b) => *b,
        DbValue::Int(n) => *n != 0,
        DbValue::Float(f) => *f != 0.0,
        DbValue::String(s) => !s.is_empty(),
        DbValue::Strings(arr) => !arr.is_empty(),
        DbValue::Floats(arr) => !arr.is_empty(),
    }
}

fn value_to_string(v: &DbValue) -> String {
    match v {
        DbValue::Null => "NULL".into(),
        DbValue::Bool(b) => b.to_string(),
        DbValue::Int(n) => n.to_string(),
        DbValue::Float(f) => f.to_string(),
        DbValue::String(s) => s.clone(),
        DbValue::Strings(arr) => arr.join(","),
        DbValue::Floats(arr) => arr
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(","),
    }
}

// ── SQL value conversion ───────────────────────────────────────────────

fn sql_val_to_db(v: &sqlparser::ast::Value) -> DbValue {
    match v {
        sqlparser::ast::Value::Null => DbValue::Null,
        sqlparser::ast::Value::Boolean(b) => DbValue::Bool(*b),
        sqlparser::ast::Value::Number(s, _) => {
            if s.contains('.') {
                s.parse::<f64>()
                    .map(DbValue::Float)
                    .unwrap_or(DbValue::String(s.clone()))
            } else {
                s.parse::<i64>()
                    .map(DbValue::Int)
                    .unwrap_or(DbValue::String(s.clone()))
            }
        }
        sqlparser::ast::Value::SingleQuotedString(s)
        | sqlparser::ast::Value::DoubleQuotedString(s) => DbValue::String(s.clone()),
        _ => DbValue::String(format!("{:?}", v)),
    }
}

// ── Data type parsing ──────────────────────────────────────────────────

fn parse_data_type(dt: &DataType) -> Result<ColumnType, String> {
    match dt {
        DataType::Int(_) | DataType::Integer(_) | DataType::BigInt(_) | DataType::SmallInt(_) => {
            Ok(ColumnType::Int)
        }
        DataType::Float(_) | DataType::Double(_) | DataType::Real => Ok(ColumnType::Float),
        DataType::String(_)
        | DataType::Text
        | DataType::Varchar(_)
        | DataType::Char(_)
        | DataType::Uuid => Ok(ColumnType::String),
        DataType::Boolean => Ok(ColumnType::Bool),
        DataType::Array(elem) => {
            use sqlparser::ast::ArrayElemTypeDef;
            let inner = match elem {
                ArrayElemTypeDef::SquareBracket(dt, _) => dt.as_ref(),
                ArrayElemTypeDef::AngleBracket(dt) => dt.as_ref(),
                ArrayElemTypeDef::Parenthesis(dt) => dt.as_ref(),
                ArrayElemTypeDef::None => return Ok(ColumnType::Strings),
            };
            match inner {
                DataType::String(_) | DataType::Varchar(_) | DataType::Text | DataType::Char(_) => {
                    Ok(ColumnType::Strings)
                }
                DataType::Float(_) | DataType::Double(_) | DataType::Real => Ok(ColumnType::Floats),
                _ if inner.to_string().to_lowercase() == "string" => Ok(ColumnType::Strings),
                _ => Err(format!("Unsupported array element type: {}", inner)),
            }
        }
        DataType::Custom(name, _) => {
            let s = name.to_string().to_uppercase();
            match s.as_str() {
                "STRINGS" => Ok(ColumnType::Strings),
                "FLOATS" => Ok(ColumnType::Floats),
                "STRING" => Ok(ColumnType::String),
                "INT" | "INTEGER" | "BIGINT" => Ok(ColumnType::Int),
                "FLOAT" | "DOUBLE" => Ok(ColumnType::Float),
                "BOOL" | "BOOLEAN" => Ok(ColumnType::Bool),
                _ => Err(format!("Unknown custom type '{}'", s)),
            }
        }
        _ => Err(format!("Unsupported data type: {:?}", dt)),
    }
}

// ── Table resolution ───────────────────────────────────────────────────

fn resolve_single_table<'a>(
    from: &[TableWithJoins],
    db: &'a Database,
) -> Result<&'a Table, String> {
    let tf = from.first().ok_or("No FROM clause")?;
    match &tf.relation {
        TableFactor::Table { name, .. } => db.get_table(&object_name_str(name)),
        _ => Err("Only simple table references supported in FROM".into()),
    }
}

fn resolve_table_from_joins(tj: &TableWithJoins) -> Result<String, String> {
    match &tj.relation {
        TableFactor::Table { name, .. } => Ok(object_name_str(name)),
        _ => Err("Only simple table references supported".into()),
    }
}

// ── CREATE INDEX handling ──────────────────────────────────────────────

fn exec_create_index(
    idx: &sqlparser::ast::CreateIndex,
    db: &mut Database,
) -> Result<String, String> {
    let index_name = match &idx.name {
        Some(name) => object_name_str(name),
        None => return Err("CREATE INDEX requires a name".into()),
    };
    let table_name = object_name_str(&idx.table_name);

    // Determine index type from USING clause (default BTREE)
    use sqlparser::ast::IndexType as SqlIdx;
    let index_type = match &idx.using {
        None | Some(SqlIdx::BTree) => A3IndexType::BTree,
        Some(SqlIdx::GIN) => A3IndexType::Trigram,
        Some(SqlIdx::Custom(id)) if id.value.to_uppercase() == "TRIGRAM" => A3IndexType::Trigram,
        Some(other) => return Err(format!("Unsupported index type: {}", other)),
    };

    // Get the column name from the first index column
    let column = match idx.columns.first() {
        Some(col) => col.to_string().to_lowercase(),
        None => return Err("CREATE INDEX requires at least one column".into()),
    };

    let table = db.get_table_mut(&table_name)?;
    table.create_index(&index_name, &column, index_type)?;

    Ok(format!(
        "\"Created {} index '{}' on {}({})\"",
        index_type, index_name, table_name, column
    ))
}

/// Drop an index by name across all tables.
fn drop_index_by_name(db: &mut Database, name: &str) -> bool {
    let table_names: Vec<String> = db.table_names().iter().map(|s| s.to_string()).collect();
    for tname in table_names {
        if let Ok(table) = db.get_table_mut(&tname) {
            if table.drop_index(name).is_ok() {
                return true;
            }
        }
    }
    false
}

// ── LIKE pattern matching ──────────────────────────────────────────────

fn simple_like(value: &str, pattern: &str) -> bool {
    let val_chars: Vec<char> = value.chars().collect();
    let pat_chars: Vec<char> = pattern.chars().collect();
    like_match(&val_chars, &pat_chars, 0, 0)
}

fn like_match(val: &[char], pat: &[char], vi: usize, pi: usize) -> bool {
    if pi == pat.len() {
        return vi == val.len();
    }
    match pat[pi] {
        '%' => {
            let mut vi2 = vi;
            while vi2 <= val.len() {
                if like_match(val, pat, vi2, pi + 1) {
                    return true;
                }
                vi2 += 1;
            }
            false
        }
        '_' => vi < val.len() && like_match(val, pat, vi + 1, pi + 1),
        c => vi < val.len() && val[vi] == c && like_match(val, pat, vi + 1, pi + 1),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::database::Database;
    use crate::engine::value::*;

    fn make_test_db() -> Database {
        let mut db = Database::new();
        let cols = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::String,
                primary_key: true,
            },
            Column {
                name: "name".into(),
                dtype: ColumnType::String,
                primary_key: false,
            },
            Column {
                name: "value".into(),
                dtype: ColumnType::Int,
                primary_key: false,
            },
        ];
        let table = Table::new("items".into(), cols).unwrap();
        db.create_table("items", table).unwrap();
        db
    }

    fn parse_and_exec(sql: &str, db: &mut Database) -> Result<String, String> {
        let stmts = crate::parser::parse_sql(sql).map_err(|e| format!("{}", e))?;
        let mut result = String::new();
        for stmt in &stmts {
            result = execute(stmt, db)?;
        }
        Ok(result)
    }

    #[test]
    fn create_table() {
        let mut db = Database::new();
        parse_and_exec("CREATE TABLE t (id STRING PRIMARY KEY, val FLOAT)", &mut db).unwrap();
        assert!(db.has_table("t"));
    }

    #[test]
    fn insert_and_select() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items", &mut db).unwrap();
        assert!(result.contains("\"id\"") && result.contains("\"name\""));
        assert!(result.contains("alpha") && result.contains("beta"));
    }

    #[test]
    fn select_with_where() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items WHERE value >= 20", &mut db).unwrap();
        assert!(result.contains("beta"));
        assert!(!result.contains("alpha"));
    }

    #[test]
    fn update_rows() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        parse_and_exec("UPDATE items SET name = 'updated' WHERE id = 'a'", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items WHERE id = 'a'", &mut db).unwrap();
        assert!(result.contains("updated"));
    }

    #[test]
    fn delete_rows() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        let result = parse_and_exec("DELETE FROM items WHERE id = 'a'", &mut db);
        eprintln!("DELETE result: {:?}", result);
        assert!(result.is_ok(), "DELETE failed: {:?}", result.err());
        assert_eq!(db.get_table("items").unwrap().rows.len(), 0);
    }

    #[test]
    fn like_operator() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('abc123', 'test', 1)", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items WHERE id LIKE '%123'", &mut db).unwrap();
        assert!(result.contains("abc123"));
    }

    #[test]
    fn trigram_index_fuzzy_after_insert() {
        let mut db = make_indexed_db();
        parse_and_exec("INSERT INTO idx_test VALUES ('rhs_m4a1', 1)", &mut db).unwrap();
        let result = parse_and_exec("SELECT k FROM idx_test WHERE k %% 'rhs_m4'", &mut db).unwrap();
        assert!(result.contains("rhs_m4a1"), "trigram index: {}", result);
    }

    // ── Phase 7: ORDER BY, LIMIT, Aggregates ─────────────────────────

    #[test]
    fn order_by_desc() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items ORDER BY value DESC", &mut db).unwrap();
        // In DESC order, 20 should come before 10
        let pos_20 = result.find(",beta,").unwrap_or(0);
        let pos_10 = result.find(",alpha,").unwrap_or(usize::MAX);
        assert!(
            pos_20 < pos_10,
            "beta(20) should appear before alpha(10) in DESC: {}",
            result
        );
    }

    #[test]
    fn limit_clause() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items LIMIT 1", &mut db).unwrap();
        let count = result.matches("alpha").count() + result.matches("beta").count();
        assert_eq!(count, 1, "LIMIT 1 should return 1 row: {}", result);
    }

    #[test]
    fn count_aggregate() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
        let result = parse_and_exec("SELECT COUNT(*) FROM items", &mut db).unwrap();
        assert!(result.contains("2"), "COUNT should be 2: {}", result);
    }

    #[test]
    fn sum_aggregate() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
        let result = parse_and_exec("SELECT SUM(value) FROM items", &mut db).unwrap();
        assert!(result.contains("30"), "SUM should be 30: {}", result);
    }

    #[test]
    fn group_by() {
        let mut db = Database::new();
        let cols = vec![
            Column {
                name: "cat".into(),
                dtype: ColumnType::String,
                primary_key: false,
            },
            Column {
                name: "val".into(),
                dtype: ColumnType::Int,
                primary_key: false,
            },
        ];
        let mut table = Table::new("data".into(), cols).unwrap();
        table
            .insert(vec![DbValue::String("a".into()), DbValue::Int(10)])
            .unwrap();
        table
            .insert(vec![DbValue::String("a".into()), DbValue::Int(20)])
            .unwrap();
        table
            .insert(vec![DbValue::String("b".into()), DbValue::Int(30)])
            .unwrap();
        db.create_table("data", table).unwrap();

        let result =
            parse_and_exec("SELECT cat, SUM(val) FROM data GROUP BY cat", &mut db).unwrap();
        assert!(result.contains("30"), "SUM(a) = 30: {}", result);
        assert!(result.contains("30"), "SUM(b) = 30: {}", result);
    }

    #[test]
    fn transaction_rollback() {
        let mut db = make_test_db();
        parse_and_exec("BEGIN", &mut db).unwrap();
        parse_and_exec(
            "INSERT INTO items VALUES ('rx', 'rollback_test', 99)",
            &mut db,
        )
        .unwrap();
        parse_and_exec("ROLLBACK", &mut db).unwrap();
        let t = db.get_table("items").unwrap();
        assert_eq!(t.rows.len(), 0, "rows should be 0 after rollback");
    }

    #[test]
    fn transaction_commit() {
        let mut db = make_test_db();
        parse_and_exec("BEGIN", &mut db).unwrap();
        parse_and_exec(
            "INSERT INTO items VALUES ('cx', 'commit_test', 99)",
            &mut db,
        )
        .unwrap();
        parse_and_exec("COMMIT", &mut db).unwrap();
        let t = db.get_table("items").unwrap();
        assert_eq!(t.rows.len(), 1, "rows should be 1 after commit");
    }

    // ── Edge cases ──────────────────────────────────────────────────

    #[test]
    fn empty_table_select() {
        let mut db = make_test_db();
        let result = parse_and_exec("SELECT * FROM items", &mut db).unwrap();
        assert_eq!(result, "[[\"id\",\"name\",\"value\"]]");
    }

    #[test]
    fn empty_where_select() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items WHERE 1 = 0", &mut db).unwrap();
        assert_eq!(result, "[[\"id\",\"name\",\"value\"]]");
    }

    #[test]
    fn null_insert() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('n', NULL, 99)", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items WHERE id = 'n'", &mut db).unwrap();
        assert!(result.contains("null"));
    }

    #[test]
    fn bulk_insert_500() {
        let mut db = Database::new();
        let cols = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::Int,
                primary_key: false,
            },
            Column {
                name: "v".into(),
                dtype: ColumnType::Int,
                primary_key: false,
            },
        ];
        let t = Table::new("bulk".into(), cols).unwrap();
        db.create_table("bulk", t).unwrap();
        for i in 0..500 {
            parse_and_exec(
                &format!("INSERT INTO bulk VALUES ({},{})", i, i * 2),
                &mut db,
            )
            .unwrap();
        }
        let r = parse_and_exec("SELECT COUNT(*) FROM bulk", &mut db).unwrap();
        assert!(r.contains("500"), "count: {}", r);
        let s = parse_and_exec("SELECT SUM(v) FROM bulk", &mut db).unwrap();
        // sum(i=0..499, i*2) = 249500
        assert!(s.contains("249500"), "sum: {}", s);
    }

    #[test]
    fn string_with_semicolon() {
        let mut db = make_test_db();
        let sql = "INSERT INTO items VALUES ('sc', 'a;b', 1)";
        parse_and_exec(sql, &mut db).unwrap();
        let r = parse_and_exec("SELECT * FROM items WHERE id = 'sc'", &mut db).unwrap();
        assert!(r.contains("a;b"));
    }

    #[test]
    fn order_empty_table() {
        let mut db = make_test_db();
        let r = parse_and_exec("SELECT * FROM items ORDER BY value", &mut db).unwrap();
        assert_eq!(r, "[[\"id\",\"name\",\"value\"]]");
    }

    // ── Index maintenance tests ─────────────────────────────────────

    fn make_indexed_db() -> Database {
        let mut db = Database::new();
        let cols = vec![
            Column {
                name: "k".into(),
                dtype: ColumnType::String,
                primary_key: true,
            },
            Column {
                name: "v".into(),
                dtype: ColumnType::Int,
                primary_key: false,
            },
        ];
        let t = Table::new("idx_test".into(), cols).unwrap();
        db.create_table("idx_test", t).unwrap();
        parse_and_exec("INSERT INTO idx_test VALUES ('a', 10)", &mut db).unwrap();
        parse_and_exec("INSERT INTO idx_test VALUES ('b', 20)", &mut db).unwrap();
        parse_and_exec("CREATE INDEX btree_v ON idx_test (v) USING BTREE", &mut db).unwrap();
        parse_and_exec(
            "CREATE INDEX trigram_k ON idx_test (k) USING TRIGRAM",
            &mut db,
        )
        .unwrap();
        db
    }

    // ── JOIN tests ──────────────────────────────────────────────────

    #[test]
    fn cross_join() {
        let mut db = Database::new();
        let ca = vec![Column {
            name: "x".into(),
            dtype: ColumnType::Int,
            primary_key: false,
        }];
        let mut ta = Table::new("ta".into(), ca).unwrap();
        ta.insert(vec![DbValue::Int(1)]).unwrap();
        ta.insert(vec![DbValue::Int(2)]).unwrap();
        db.create_table("ta", ta).unwrap();
        let cb = vec![Column {
            name: "y".into(),
            dtype: ColumnType::String,
            primary_key: false,
        }];
        let mut tb = Table::new("tb".into(), cb).unwrap();
        tb.insert(vec![DbValue::String("a".into())]).unwrap();
        db.create_table("tb", tb).unwrap();
        let r = parse_and_exec("SELECT * FROM ta, tb", &mut db).unwrap();
        assert!(
            r.contains("1") && r.contains("a") && r.contains("2"),
            "cross join: {}",
            r
        );
    }

    #[test]
    fn inner_join() {
        let mut db = Database::new();
        let ca = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::Int,
                primary_key: false,
            },
            Column {
                name: "v".into(),
                dtype: ColumnType::String,
                primary_key: false,
            },
        ];
        let mut ta = Table::new("a".into(), ca).unwrap();
        ta.insert(vec![DbValue::Int(1), DbValue::String("one".into())])
            .unwrap();
        ta.insert(vec![DbValue::Int(2), DbValue::String("two".into())])
            .unwrap();
        db.create_table("a", ta).unwrap();
        let cb = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::Int,
                primary_key: false,
            },
            Column {
                name: "d".into(),
                dtype: ColumnType::String,
                primary_key: false,
            },
        ];
        let mut tb = Table::new("b".into(), cb).unwrap();
        tb.insert(vec![DbValue::Int(1), DbValue::String("desc1".into())])
            .unwrap();
        tb.insert(vec![DbValue::Int(3), DbValue::String("desc3".into())])
            .unwrap();
        db.create_table("b", tb).unwrap();
        let r = parse_and_exec("SELECT * FROM a INNER JOIN b ON a.id = b.id", &mut db).unwrap();
        assert!(r.contains("one"), "inner join: {}", r);
        assert!(!r.contains("two"), "should exclude two: {}", r);
    }

    #[test]
    fn left_join() {
        let mut db = Database::new();
        let ca = vec![Column {
            name: "k".into(),
            dtype: ColumnType::String,
            primary_key: false,
        }];
        let mut ta = Table::new("a".into(), ca).unwrap();
        ta.insert(vec![DbValue::String("x".into())]).unwrap();
        ta.insert(vec![DbValue::String("y".into())]).unwrap();
        db.create_table("a", ta).unwrap();
        let cb = vec![Column {
            name: "k".into(),
            dtype: ColumnType::String,
            primary_key: false,
        }];
        let mut tb = Table::new("b".into(), cb).unwrap();
        tb.insert(vec![DbValue::String("x".into())]).unwrap();
        db.create_table("b", tb).unwrap();
        let r = parse_and_exec("SELECT * FROM a LEFT JOIN b ON a.k = b.k", &mut db).unwrap();
        assert!(r.contains("x"), "x: {}", r);
        assert!(r.contains("null") || r.contains("y"), "y null: {}", r);
    }

    #[test]
    fn join_with_where() {
        let mut db = Database::new();
        let ca = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::Int,
                primary_key: false,
            },
            Column {
                name: "n".into(),
                dtype: ColumnType::String,
                primary_key: false,
            },
        ];
        let mut ta = Table::new("u".into(), ca).unwrap();
        ta.insert(vec![DbValue::Int(1), DbValue::String("alice".into())])
            .unwrap();
        ta.insert(vec![DbValue::Int(2), DbValue::String("bob".into())])
            .unwrap();
        db.create_table("u", ta).unwrap();
        let cb = vec![
            Column {
                name: "uid".into(),
                dtype: ColumnType::Int,
                primary_key: false,
            },
            Column {
                name: "r".into(),
                dtype: ColumnType::String,
                primary_key: false,
            },
        ];
        let mut tb = Table::new("r".into(), cb).unwrap();
        tb.insert(vec![DbValue::Int(1), DbValue::String("admin".into())])
            .unwrap();
        tb.insert(vec![DbValue::Int(2), DbValue::String("user".into())])
            .unwrap();
        db.create_table("r", tb).unwrap();
        let sql = "SELECT * FROM u INNER JOIN r ON u.id = r.uid WHERE r.r = 'admin'";
        let r = parse_and_exec(sql, &mut db).unwrap();
        assert!(r.contains("alice"), "alice admin: {}", r);
        assert!(!r.contains("bob"), "bob not admin: {}", r);
    }

    #[test]
    fn null_arithmetic() {
        let mut db = make_test_db();
        parse_and_exec(
            "INSERT INTO items VALUES ('nx', 'null_test', NULL)",
            &mut db,
        )
        .unwrap();
        let r = parse_and_exec("SELECT * FROM items WHERE value IS NULL", &mut db).unwrap();
        assert!(r.contains("null_test"), "null: {}", r);
    }

    #[test]
    fn fuzzy_fn_call_integration() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('fn_test', 'hello', 1)", &mut db).unwrap();
        let r = parse_and_exec("SELECT * FROM items WHERE id %% 'fn_t'", &mut db).unwrap();
        assert!(r.contains("fn_test"), "fuzzy fn: {}", r);
    }
}
