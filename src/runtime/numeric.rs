use super::{ApexDouble, Value, integer_overflow, invalid_runtime_operands, runtime_exception};
use crate::{
    ast::BinaryOperator,
    diagnostic::Diagnostic,
    hir::{CheckedBinaryOperation, CheckedUnaryOperation, NumericKind},
    span::Span,
};
use rust_decimal::Decimal;

pub(super) fn apply_binary(
    operation: CheckedBinaryOperation,
    left: Value,
    right: Value,
    span: Span,
) -> Result<Value, Diagnostic> {
    if matches!(left, Value::Null(_)) || matches!(right, Value::Null(_)) {
        return Err(runtime_exception(
            "NullPointerException",
            "operator cannot be applied to null at runtime",
            span,
        ));
    }

    match operation {
        CheckedBinaryOperation::StringConcat => Err(Diagnostic::new(
            "String concatenation requires runtime rendering",
            span,
        )),
        CheckedBinaryOperation::Numeric { operator, kind } => {
            apply_numeric_binary(operator, kind, left, right, span)
        }
        CheckedBinaryOperation::BooleanBitwise(operator) => {
            let (Value::Boolean(left), Value::Boolean(right)) = (left, right) else {
                return Err(invalid_runtime_operands(span));
            };
            Ok(Value::Boolean(match operator {
                BinaryOperator::BitwiseAnd => left & right,
                BinaryOperator::BitwiseOr => left | right,
                BinaryOperator::BitwiseXor => left ^ right,
                _ => return Err(invalid_runtime_operands(span)),
            }))
        }
        CheckedBinaryOperation::Integral { operator, kind } => {
            apply_integral_binary(operator, kind, left, right, span)
        }
        CheckedBinaryOperation::Shift { operator, kind } => {
            apply_shift(operator, kind, left, right, span)
        }
    }
}

pub(super) fn apply_unary(
    operation: CheckedUnaryOperation,
    value: Value,
    span: Span,
) -> Result<Value, Diagnostic> {
    if matches!(value, Value::Null(_)) {
        return Err(runtime_exception(
            "NullPointerException",
            "expected non-null numeric value at runtime",
            span,
        ));
    }
    match operation {
        CheckedUnaryOperation::Positive(kind) => convert_numeric(value, kind, span),
        CheckedUnaryOperation::Negate(NumericKind::Integer) => {
            let value = integer(value, span)?;
            i32::try_from(value)
                .ok()
                .and_then(i32::checked_neg)
                .map(|result| Value::Integer(i64::from(result)))
                .ok_or_else(|| integer_overflow(span))
        }
        CheckedUnaryOperation::Negate(NumericKind::Long) => long(value, span)?
            .checked_neg()
            .map(Value::Long)
            .ok_or_else(|| integer_overflow(span)),
        CheckedUnaryOperation::Negate(NumericKind::Decimal) => decimal(value, span)?
            .checked_mul(Decimal::NEGATIVE_ONE)
            .map(Value::Decimal)
            .ok_or_else(|| decimal_overflow(span)),
        CheckedUnaryOperation::Negate(NumericKind::Double) => {
            finite_double(-double(value, span)?, span)
        }
        CheckedUnaryOperation::BitwiseNot(NumericKind::Integer) => {
            let value =
                i32::try_from(integer(value, span)?).map_err(|_| invalid_runtime_operands(span))?;
            Ok(Value::Integer(i64::from(!value)))
        }
        CheckedUnaryOperation::BitwiseNot(NumericKind::Long) => {
            Ok(Value::Long(!long(value, span)?))
        }
        CheckedUnaryOperation::BitwiseNot(NumericKind::Decimal | NumericKind::Double) => {
            Err(invalid_runtime_operands(span))
        }
    }
}

pub(super) fn increment(
    kind: NumericKind,
    value: Value,
    delta: i32,
    span: Span,
) -> Result<Value, Diagnostic> {
    let one = match kind {
        NumericKind::Integer => Value::Integer(i64::from(delta)),
        NumericKind::Long => Value::Long(i64::from(delta)),
        NumericKind::Decimal => return Err(invalid_runtime_operands(span)),
        NumericKind::Double => Value::Double(ApexDouble(f64::from(delta))),
    };
    apply_numeric_binary(BinaryOperator::Add, kind, value, one, span)
}

fn apply_numeric_binary(
    operator: BinaryOperator,
    kind: NumericKind,
    left: Value,
    right: Value,
    span: Span,
) -> Result<Value, Diagnostic> {
    match kind {
        NumericKind::Integer => apply_integer_binary(operator, left, right, span),
        NumericKind::Long => apply_long_binary(operator, left, right, span),
        NumericKind::Decimal => apply_decimal_binary(operator, left, right, span),
        NumericKind::Double => apply_double_binary(operator, left, right, span),
    }
}

fn apply_integer_binary(
    operator: BinaryOperator,
    left: Value,
    right: Value,
    span: Span,
) -> Result<Value, Diagnostic> {
    let left = i32::try_from(integer(left, span)?).map_err(|_| invalid_runtime_operands(span))?;
    let right = i32::try_from(integer(right, span)?).map_err(|_| invalid_runtime_operands(span))?;
    let value = match operator {
        BinaryOperator::Add => left.checked_add(right),
        BinaryOperator::Subtract => left.checked_sub(right),
        BinaryOperator::Multiply => left.checked_mul(right),
        BinaryOperator::Divide if right == 0 => return Err(division_by_zero(span)),
        BinaryOperator::Divide => left.checked_div(right),
        BinaryOperator::Remainder if right == 0 => return Err(remainder_by_zero(span)),
        BinaryOperator::Remainder => left.checked_rem(right),
        _ => return Err(invalid_runtime_operands(span)),
    }
    .ok_or_else(|| integer_overflow(span))?;
    Ok(Value::Integer(i64::from(value)))
}

fn apply_long_binary(
    operator: BinaryOperator,
    left: Value,
    right: Value,
    span: Span,
) -> Result<Value, Diagnostic> {
    let left = long(left, span)?;
    let right = long(right, span)?;
    let value = match operator {
        BinaryOperator::Add => left.checked_add(right),
        BinaryOperator::Subtract => left.checked_sub(right),
        BinaryOperator::Multiply => left.checked_mul(right),
        BinaryOperator::Divide if right == 0 => return Err(division_by_zero(span)),
        BinaryOperator::Divide => left.checked_div(right),
        BinaryOperator::Remainder if right == 0 => return Err(remainder_by_zero(span)),
        BinaryOperator::Remainder => left.checked_rem(right),
        _ => return Err(invalid_runtime_operands(span)),
    }
    .ok_or_else(|| integer_overflow(span))?;
    Ok(Value::Long(value))
}

fn apply_decimal_binary(
    operator: BinaryOperator,
    left: Value,
    right: Value,
    span: Span,
) -> Result<Value, Diagnostic> {
    let left = decimal(left, span)?;
    let right = decimal(right, span)?;
    if right.is_zero() && matches!(operator, BinaryOperator::Divide | BinaryOperator::Remainder) {
        return Err(if operator == BinaryOperator::Divide {
            division_by_zero(span)
        } else {
            remainder_by_zero(span)
        });
    }
    let value = match operator {
        BinaryOperator::Add => left.checked_add(right),
        BinaryOperator::Subtract => left.checked_sub(right),
        BinaryOperator::Multiply => left.checked_mul(right),
        BinaryOperator::Divide => left.checked_div(right),
        BinaryOperator::Remainder => left.checked_rem(right),
        _ => return Err(invalid_runtime_operands(span)),
    }
    .ok_or_else(|| decimal_overflow(span))?;
    Ok(Value::Decimal(value))
}

fn apply_double_binary(
    operator: BinaryOperator,
    left: Value,
    right: Value,
    span: Span,
) -> Result<Value, Diagnostic> {
    let left = double(left, span)?;
    let right = double(right, span)?;
    if right == 0.0 && matches!(operator, BinaryOperator::Divide | BinaryOperator::Remainder) {
        return Err(if operator == BinaryOperator::Divide {
            division_by_zero(span)
        } else {
            remainder_by_zero(span)
        });
    }
    let value = match operator {
        BinaryOperator::Add => left + right,
        BinaryOperator::Subtract => left - right,
        BinaryOperator::Multiply => left * right,
        BinaryOperator::Divide => left / right,
        BinaryOperator::Remainder => left % right,
        _ => return Err(invalid_runtime_operands(span)),
    };
    finite_double(value, span)
}

fn apply_integral_binary(
    operator: BinaryOperator,
    kind: NumericKind,
    left: Value,
    right: Value,
    span: Span,
) -> Result<Value, Diagnostic> {
    match kind {
        NumericKind::Integer => {
            let left =
                i32::try_from(integer(left, span)?).map_err(|_| invalid_runtime_operands(span))?;
            let right =
                i32::try_from(integer(right, span)?).map_err(|_| invalid_runtime_operands(span))?;
            Ok(Value::Integer(i64::from(match operator {
                BinaryOperator::BitwiseAnd => left & right,
                BinaryOperator::BitwiseOr => left | right,
                BinaryOperator::BitwiseXor => left ^ right,
                _ => return Err(invalid_runtime_operands(span)),
            })))
        }
        NumericKind::Long => {
            let left = long(left, span)?;
            let right = long(right, span)?;
            Ok(Value::Long(match operator {
                BinaryOperator::BitwiseAnd => left & right,
                BinaryOperator::BitwiseOr => left | right,
                BinaryOperator::BitwiseXor => left ^ right,
                _ => return Err(invalid_runtime_operands(span)),
            }))
        }
        NumericKind::Decimal | NumericKind::Double => Err(invalid_runtime_operands(span)),
    }
}

fn apply_shift(
    operator: BinaryOperator,
    kind: NumericKind,
    left: Value,
    right: Value,
    span: Span,
) -> Result<Value, Diagnostic> {
    let distance = integer(right, span)?;
    match kind {
        NumericKind::Integer => {
            let left =
                i32::try_from(integer(left, span)?).map_err(|_| invalid_runtime_operands(span))?;
            let distance = u32::try_from(distance & 31).expect("masked shift distance");
            Ok(Value::Integer(i64::from(match operator {
                BinaryOperator::ShiftLeft => left.wrapping_shl(distance),
                BinaryOperator::ShiftRight => left.wrapping_shr(distance),
                BinaryOperator::UnsignedShiftRight => ((left as u32).wrapping_shr(distance)) as i32,
                _ => return Err(invalid_runtime_operands(span)),
            })))
        }
        NumericKind::Long => {
            let left = long(left, span)?;
            let distance = u32::try_from(distance & 63).expect("masked shift distance");
            Ok(Value::Long(match operator {
                BinaryOperator::ShiftLeft => left.wrapping_shl(distance),
                BinaryOperator::ShiftRight => left.wrapping_shr(distance),
                BinaryOperator::UnsignedShiftRight => ((left as u64).wrapping_shr(distance)) as i64,
                _ => return Err(invalid_runtime_operands(span)),
            }))
        }
        NumericKind::Decimal | NumericKind::Double => Err(invalid_runtime_operands(span)),
    }
}

fn convert_numeric(value: Value, kind: NumericKind, span: Span) -> Result<Value, Diagnostic> {
    match kind {
        NumericKind::Integer => {
            let value =
                i32::try_from(integer(value, span)?).map_err(|_| invalid_runtime_operands(span))?;
            Ok(Value::Integer(i64::from(value)))
        }
        NumericKind::Long => Ok(Value::Long(long(value, span)?)),
        NumericKind::Decimal => Ok(Value::Decimal(decimal(value, span)?)),
        NumericKind::Double => finite_double(double(value, span)?, span),
    }
}

fn integer(value: Value, span: Span) -> Result<i64, Diagnostic> {
    match value {
        Value::Integer(value) => Ok(value),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn long(value: Value, span: Span) -> Result<i64, Diagnostic> {
    match value {
        Value::Integer(value) | Value::Long(value) => Ok(value),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn decimal(value: Value, span: Span) -> Result<Decimal, Diagnostic> {
    match value {
        Value::Integer(value) | Value::Long(value) => Ok(Decimal::from(value)),
        Value::Decimal(value) => Ok(value),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn double(value: Value, span: Span) -> Result<f64, Diagnostic> {
    match value {
        Value::Integer(value) | Value::Long(value) => Ok(value as f64),
        Value::Decimal(value) => value
            .normalize()
            .to_string()
            .parse::<f64>()
            .map_err(|_| invalid_runtime_operands(span)),
        Value::Double(value) => Ok(value.get()),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn finite_double(value: f64, span: Span) -> Result<Value, Diagnostic> {
    ApexDouble::new(value)
        .map(Value::Double)
        .ok_or_else(|| runtime_exception("MathException", "Double arithmetic overflow", span))
}

fn division_by_zero(span: Span) -> Diagnostic {
    runtime_exception("MathException", "division by zero", span)
}

fn remainder_by_zero(span: Span) -> Diagnostic {
    runtime_exception("MathException", "remainder by zero", span)
}

fn decimal_overflow(span: Span) -> Diagnostic {
    runtime_exception("MathException", "Decimal arithmetic overflow", span)
}
