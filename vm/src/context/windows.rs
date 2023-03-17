#[cfg(target_arch = "x86_64")]
mod x86_64;

#[cfg(not(target_arch = "x86_64"))]
std::compile_error!("Only x86-64 is supported for Windows at this time");
