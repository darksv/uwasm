use crate::interpreter::VmStack;

#[derive(Debug)]
pub enum EvaluationError {
    EmptyStack,
}

pub trait Operand: Copy {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError>;

    fn push(stack: &mut VmStack, value: Self);
}

impl Operand for i32 {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError> {
        stack.pop_i32().ok_or(EvaluationError::EmptyStack)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_i32(value)
    }
}

impl Operand for u32 {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError> {
        stack.pop_i32().map(|s| s as u32).ok_or(EvaluationError::EmptyStack)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_i32(value as i32)
    }
}

impl Operand for i64 {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError> {
        stack.pop_i64().ok_or(EvaluationError::EmptyStack)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_i64(value)
    }
}

impl Operand for u64 {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError> {
        stack.pop_i64().map(|s| s as u64).ok_or(EvaluationError::EmptyStack)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_i64(value as i64)
    }
}

impl Operand for f32 {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError> {
        stack.pop_f32().ok_or(EvaluationError::EmptyStack)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_f32(value)
    }
}

impl Operand for f64 {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError> {
        stack.pop_f64().ok_or(EvaluationError::EmptyStack)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_f64(value)
    }
}

impl Operand for bool {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError> {
        stack.pop_i32().map(|s| s != 0).ok_or(EvaluationError::EmptyStack)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_i32(value as i32)
    }
}