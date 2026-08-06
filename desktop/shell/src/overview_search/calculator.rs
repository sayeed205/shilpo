pub fn evaluate_expression(expr: &str) -> Option<String> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut ns = fasteval::EmptyNamespace;
    let val = fasteval::ez_eval(trimmed, &mut ns).ok()?;

    if val.is_nan() {
        Some("NaN".to_string())
    } else if val.is_infinite() {
        if val.is_sign_positive() {
            Some("∞".to_string())
        } else {
            Some("-∞".to_string())
        }
    } else {
        // Format nicely: strip trailing zeros if integer value
        if val.fract() == 0.0 && val.abs() < 1e15 {
            Some(format!("{}", val as i64))
        } else {
            Some(
                format!("{:.6}", val)
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .to_string(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculator_evaluation() {
        assert_eq!(evaluate_expression("2 + 2"), Some("4".to_string()));
        assert_eq!(evaluate_expression("10 * (5 - 3)"), Some("20".to_string()));
        assert_eq!(evaluate_expression("15 / 2"), Some("7.5".to_string()));
        assert_eq!(evaluate_expression("invalid input"), None);
    }
}
