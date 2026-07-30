use super::*;

fn eval_cmd(name: &str, args: &[DbValue]) -> Result<DbValue, String> {
    eval_command(name, args)
}

#[test]
fn test_pi() {
    assert_eq!(eval_cmd("pi", &[]).unwrap(), DbValue::Float(std::f64::consts::PI));
}
#[test]
fn test_true_false_nil() {
    assert_eq!(eval_cmd("true", &[]).unwrap(), DbValue::Bool(true));
    assert_eq!(eval_cmd("false", &[]).unwrap(), DbValue::Bool(false));
    assert_eq!(eval_cmd("nil", &[]).unwrap(), DbValue::Null);
}

#[test]
fn test_sqrt() {
    assert_eq!(eval_cmd("sqrt", &[DbValue::Int(25)]).unwrap(), DbValue::Float(5.0));
}
#[test]
fn test_abs() {
    assert_eq!(eval_cmd("abs", &[DbValue::Int(-5)]).unwrap(), DbValue::Float(5.0));
}

#[test]
fn test_round_floor_ceil() {
    assert_eq!(eval_cmd("round", &[DbValue::Float(3.7)]).unwrap(), DbValue::Int(4));
    assert_eq!(eval_cmd("floor", &[DbValue::Float(3.7)]).unwrap(), DbValue::Int(3));
    assert_eq!(eval_cmd("ceil", &[DbValue::Float(3.2)]).unwrap(), DbValue::Int(4));
}

#[test]
fn test_trunc() {
    assert_eq!(eval_cmd("trunc", &[DbValue::Float(3.7)]).unwrap(), DbValue::Int(3));
    assert_eq!(eval_cmd("trunc", &[DbValue::Float(-3.7)]).unwrap(), DbValue::Int(-3));
}

#[test]
fn test_sign() {
    assert_eq!(eval_cmd("sign", &[DbValue::Int(-5)]).unwrap(), DbValue::Int(-1));
    assert_eq!(eval_cmd("sign", &[DbValue::Int(0)]).unwrap(), DbValue::Int(0));
    assert_eq!(eval_cmd("sign", &[DbValue::Int(5)]).unwrap(), DbValue::Int(1));
}

#[test]
fn test_min_max() {
    assert_eq!(
        eval_cmd("min", &[DbValue::Int(3), DbValue::Int(7)]).unwrap(),
        DbValue::Int(3)
    );
    assert_eq!(
        eval_cmd("max", &[DbValue::Int(3), DbValue::Int(7)]).unwrap(),
        DbValue::Int(7)
    );
}

#[test]
fn test_atan2() {
    assert_eq!(
        eval_cmd("atan2", &[DbValue::Int(0), DbValue::Int(1)]).unwrap(),
        DbValue::Float(0.0)
    );
}

#[test]
fn test_clamp() {
    let r = eval_cmd(
        "clamp",
        &[DbValue::Float(5.0), DbValue::Float(0.0), DbValue::Float(3.0)],
    )
    .unwrap();
    assert_eq!(r, DbValue::Float(3.0));
}

#[test]
fn test_str() {
    assert_eq!(
        eval_cmd("str", &[DbValue::Int(42)]).unwrap(),
        DbValue::String("42".into())
    );
}

#[test]
fn test_toupper_tolower() {
    assert_eq!(
        eval_cmd("toupper", &[DbValue::String("hello".into())]).unwrap(),
        DbValue::String("HELLO".into())
    );
    assert_eq!(
        eval_cmd("tolower", &[DbValue::String("HELLO".into())]).unwrap(),
        DbValue::String("hello".into())
    );
}

#[test]
fn test_typename() {
    assert_eq!(
        eval_cmd("typename", &[DbValue::Int(42)]).unwrap(),
        DbValue::String("SCALAR".into())
    );
    assert_eq!(
        eval_cmd("typename", &[DbValue::String("x".into())]).unwrap(),
        DbValue::String("STRING".into())
    );
}

#[test]
fn test_trim() {
    assert_eq!(
        eval_cmd("trim", &[DbValue::String("  hello  ".into())]).unwrap(),
        DbValue::String("hello".into())
    );
}

#[test]
fn test_replace() {
    assert_eq!(
        eval_cmd(
            "replace",
            &[
                DbValue::String("hello world".into()),
                DbValue::String("world".into()),
                DbValue::String("there".into())
            ]
        )
        .unwrap(),
        DbValue::String("hello there".into())
    );
}

#[test]
fn test_find() {
    assert_eq!(
        eval_cmd("find", &[DbValue::String("hello".into()), DbValue::String("ll".into())]).unwrap(),
        DbValue::Int(2)
    );
    assert_eq!(
        eval_cmd(
            "find",
            &[DbValue::String("hello".into()), DbValue::String("xyz".into())]
        )
        .unwrap(),
        DbValue::Int(-1)
    );
}

#[test]
fn test_parsenumber() {
    assert_eq!(
        eval_cmd("parsenumber", &[DbValue::String("42".into())]).unwrap(),
        DbValue::Int(42)
    );
    #[allow(clippy::approx_constant)]
    let r = eval_cmd("parsenumber", &[DbValue::String("3.14".into())]).unwrap();
    match r {
        #[allow(clippy::approx_constant)]
        DbValue::Float(f) => assert!((f - 3.14).abs() < 0.001),
        _ => panic!("expected Float"),
    }
}

#[test]
fn test_count() {
    assert_eq!(
        eval_cmd("count", &[DbValue::String("hello".into())]).unwrap(),
        DbValue::Int(5)
    );
    assert_eq!(
        eval_cmd("count", &[DbValue::Strings(vec!["a".into(), "b".into()])]).unwrap(),
        DbValue::Int(2)
    );
}

#[test]
fn test_isnil() {
    assert_eq!(eval_cmd("isnil", &[DbValue::Null]).unwrap(), DbValue::Bool(true));
    assert_eq!(eval_cmd("isnil", &[DbValue::Int(0)]).unwrap(), DbValue::Bool(false));
}

#[test]
fn test_isequalto() {
    assert_eq!(
        eval_cmd("isequalto", &[DbValue::Int(1), DbValue::Int(1)]).unwrap(),
        DbValue::Bool(true)
    );
    assert_eq!(
        eval_cmd("isequalto", &[DbValue::Int(1), DbValue::Int(2)]).unwrap(),
        DbValue::Bool(false)
    );
}

#[test]
fn test_select() {
    let arr = DbValue::Strings(vec!["a".into(), "b".into(), "c".into()]);
    assert_eq!(
        eval_cmd("select", &[arr.clone(), DbValue::Int(0)]).unwrap(),
        DbValue::String("a".into())
    );
    assert_eq!(
        eval_cmd("select", &[arr.clone(), DbValue::Int(1)]).unwrap(),
        DbValue::String("b".into())
    );
    assert_eq!(eval_cmd("select", &[arr, DbValue::Int(10)]).unwrap(), DbValue::Null);
}

#[test]
fn test_select_string() {
    assert_eq!(
        eval_cmd("select", &[DbValue::String("hello".into()), DbValue::Int(0)]).unwrap(),
        DbValue::String("h".into())
    );
}

#[test]
fn test_in() {
    let arr = DbValue::Strings(vec!["a".into(), "b".into(), "c".into()]);
    assert_eq!(
        eval_cmd("in", &[DbValue::String("a".into()), arr.clone()]).unwrap(),
        DbValue::Bool(true)
    );
    assert_eq!(
        eval_cmd("in", &[DbValue::String("z".into()), arr]).unwrap(),
        DbValue::Bool(false)
    );
}

#[test]
fn test_pushback() {
    let arr = DbValue::Strings(vec!["a".into(), "b".into()]);
    let r = eval_cmd("pushback", &[arr, DbValue::String("c".into())]).unwrap();
    assert_eq!(r, DbValue::Strings(vec!["a".into(), "b".into(), "c".into()]));
}

#[test]
fn test_vectormagnitude() {
    let v = DbValue::Floats(vec![3.0, 4.0]);
    let r = eval_cmd("vectormagnitude", &[v]).unwrap();
    assert_eq!(r, DbValue::Float(5.0));
}

#[test]
fn test_unknown_command_returns_nil() {
    assert_eq!(eval_cmd("zzz_nonexistent_cmd", &[]).unwrap(), DbValue::Null);
    assert_eq!(eval_cmd("createvehicle", &[]).unwrap(), DbValue::Null);
}
