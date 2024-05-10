use crate::interpreter::{Serializer, VmStack};

#[derive(Debug)]
pub enum EvaluationError {
    EmptyStack,
}

pub trait Operand: Copy {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError>;

    fn push(stack: &mut VmStack, value: Self);

    fn write_to(&self, serializer: &mut Serializer);
}

impl Operand for i32 {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError> {
        stack.pop_i32().ok_or(EvaluationError::EmptyStack)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_i32(value)
    }

    fn write_to(&self, serializer: &mut Serializer) {
        serializer.write_bytes(&self.to_ne_bytes());
    }
}

impl Operand for u32 {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError> {
        stack.pop_i32().map(|s| s as u32).ok_or(EvaluationError::EmptyStack)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_i32(value as i32)
    }

    fn write_to(&self, serializer: &mut Serializer) {
        serializer.write_bytes(&self.to_ne_bytes());
    }
}

impl Operand for i64 {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError> {
        stack.pop_i64().ok_or(EvaluationError::EmptyStack)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_i64(value)
    }

    fn write_to(&self, serializer: &mut Serializer) {
        serializer.write_bytes(&self.to_ne_bytes());
    }
}

impl Operand for u64 {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError> {
        stack.pop_i64().map(|s| s as u64).ok_or(EvaluationError::EmptyStack)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_i64(value as i64)
    }

    fn write_to(&self, serializer: &mut Serializer) {
        serializer.write_bytes(&self.to_ne_bytes());
    }
}

impl Operand for f32 {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError> {
        stack.pop_f32().ok_or(EvaluationError::EmptyStack)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_f32(value)
    }

    fn write_to(&self, serializer: &mut Serializer) {
        serializer.write_bytes(&self.to_ne_bytes());
    }
}

impl Operand for f64 {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError> {
        stack.pop_f64().ok_or(EvaluationError::EmptyStack)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_f64(value)
    }

    fn write_to(&self, serializer: &mut Serializer) {
        serializer.write_bytes(&self.to_ne_bytes());
    }
}

impl Operand for bool {
    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError> {
        stack.pop_i32().map(|s| s != 0).ok_or(EvaluationError::EmptyStack)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_i32(value as i32)
    }

    fn write_to(&self, serializer: &mut Serializer) {
        serializer.write_bytes(&[*self as u8]);
    }
}