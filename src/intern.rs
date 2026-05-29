pub fn intern(value: impl Into<String>) -> &'static str {
    Box::leak(value.into().into_boxed_str())
}
