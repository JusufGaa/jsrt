/// Value tags, mirroring the tag enum in QuickJS `quickjs.h`.
///
/// Negative tags are refcounted heap values; non-negative tags are
/// immediates stored inline in the [`JsValue`](super::JsValue) payload.
#[repr(i64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsTag {
    // -- negative (refcounted) tags --
    // `JS_TAG_FIRST` == `JS_TAG_BIG_DECIMAL` == -11 in QuickJS; use
    // `JsTag::BigDecimal` (or the constant below) as the lower bound.
    BigDecimal       = -11, // JS_TAG_BIG_DECIMAL / JS_TAG_FIRST
    BigInt           = -10, // JS_TAG_BIG_INT
    BigFloat         = -9,  // JS_TAG_BIG_FLOAT
    Symbol           = -8,  // JS_TAG_SYMBOL
    String           = -7,  // JS_TAG_STRING
    Module           = -3,  // JS_TAG_MODULE
    FunctionBytecode = -2,  // JS_TAG_FUNCTION_BYTECODE
    Object           = -1,  // JS_TAG_OBJECT

    // -- immediate tags --
    Int           = 0,
    Bool          = 1,
    Null          = 2,
    Undefined     = 3,
    Uninitialized = 4, // JS_TAG_UNINITIALIZED
    CatchOffset   = 5, // JS_TAG_CATCH_OFFSET
    Exception     = 6, // JS_TAG_EXCEPTION
    Float64       = 7,
}
