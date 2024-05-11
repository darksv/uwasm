use crate::interpreter::{Serializer, VmStack};
use crate::parser::TypeKind;

#[derive(Debug)]
pub enum EvaluationError {
    EmptyStack,
}

pub trait Operand: Copy {
    const TYPE: TypeKind;

    fn pop(stack: &mut VmStack) -> Result<Self, EvaluationError>;

    fn push(stack: &mut VmStack, value: Self);

    fn write_to(&self, serializer: &mut Serializer);
}

impl Operand for i32 {
    const TYPE: TypeKind = TypeKind::I32;

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
    const TYPE: TypeKind = TypeKind::I32;

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
    const TYPE: TypeKind = TypeKind::I64;

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
    const TYPE: TypeKind = TypeKind::I64;

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
    const TYPE: TypeKind = TypeKind::F32;

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
    const TYPE: TypeKind = TypeKind::F64;
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
    const TYPE: TypeKind = TypeKind::Void;

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