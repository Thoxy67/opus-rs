use std::{env, fs, path};

const OPUS_TAG_VERSION: &str = "v1.5.2";

fn main() {
	let root = path::PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
	let opus_path = root.join("opus");

	// Clone Opus repository if it doesn't exist
	ensure_opus_source(&opus_path);

	build_opus_from_source(&opus_path);

	let bindings = bindgen::Builder::default()
		.header(opus_path.join("include/opus.h").to_string_lossy().into_owned())
		.header(opus_path.join("include/opus_custom.h").to_string_lossy().into_owned())
		.header(opus_path.join("include/opus_multistream.h").to_string_lossy().into_owned())
		.header(opus_path.join("include/opus_projection.h").to_string_lossy().into_owned())
		.clang_arg("-I")
		.clang_arg(opus_path.join("include").to_string_lossy())
		.clang_arg("-I")
		.clang_arg(opus_path.join("src").to_string_lossy())
		.derive_debug(true)
		.derive_default(true)
		// .allowlist_recursively(false)
		.generate()
		.expect("Unable to generate bindings");

	let output_dir = path::PathBuf::from(env::var("OUT_DIR").unwrap());

	bindings.write_to_file(output_dir.join("bindings.rs")).expect("Couldn't write bindings!");
}

fn build_opus_from_source(opus_path: &path::Path) {
	let mut build = cc::Build::new();

	build
		.include(opus_path.join("include"))
		.include(opus_path.join("src"))
		.include(opus_path.join("celt"))
		.include(opus_path.join("silk"))
		.include(opus_path.join("silk/float"))
		.define("OPUS_BUILD", None)
		.define("USE_ALLOCA", None)
		.define("HAVE_LRINT", None)
		.define("HAVE_LRINTF", None)
		.opt_level(2);

	// Enable SIMD optimizations based on target architecture
	let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
	let target_feature = env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();
	let enable_simd = env::var("CARGO_SIMD_ENABLE").unwrap_or_default();

	match enable_simd.as_str() {
		"true" | "1" => {
			match target_arch.as_str() {
				"x86" | "x86_64" => {
					// Enable SSE/AVX for x86/x64
					if target_feature.contains("avx") || cfg!(target_feature = "avx") {
						build.define("OPUS_X86_MAY_HAVE_AVX", None);
						println!("cargo:warning=Enabling AVX optimizations for Opus");
					}
					if target_feature.contains("sse4.1") || cfg!(target_feature = "sse4.1") {
						build.define("OPUS_X86_MAY_HAVE_SSE4_1", None);
						println!("cargo:warning=Enabling SSE4.1 optimizations for Opus");
					}
					if target_feature.contains("sse2") || cfg!(target_feature = "sse2") {
						build.define("OPUS_X86_MAY_HAVE_SSE2", None);
						println!("cargo:warning=Enabling SSE2 optimizations for Opus");
					}
					// Always presume SSE is available on x86_64
					build.define("OPUS_X86_PRESUME_SSE", None);
				}
				"arm" | "aarch64" => {
					// Enable NEON for ARM/ARM64
					if target_feature.contains("neon") || target_arch == "aarch64" {
						build.define("OPUS_ARM_MAY_HAVE_NEON", None);
						build.define("OPUS_ARM_MAY_HAVE_NEON_INTR", None);
						println!("cargo:warning=Enabling NEON optimizations for Opus");
					}
				}
				_ => {}
			}
		}

		_ => {}
	}

	// Only apply GCC/Clang warning flags when not using MSVC
	let compiler = build.get_compiler();
	if !compiler.is_like_msvc() {
		build
			.flag("-Wno-unused-variable")
			.flag("-Wno-unused-parameter")
			.flag("-Wno-unused-but-set-variable")
			.flag("-Wno-maybe-uninitialized")
			.flag("-Wno-sign-compare")
			.flag("-Wno-pragmas");
	}

	add_c_files(&mut build, &opus_path.join("src"));
	add_c_files(&mut build, &opus_path.join("celt"));
	add_c_files(&mut build, &opus_path.join("silk"));
	add_c_files(&mut build, &opus_path.join("silk/float"));

	build.compile("opus");
}

fn add_c_files(build: &mut cc::Build, dir: &path::Path) {
	if let Ok(entries) = fs::read_dir(dir) {
		for entry in entries.flatten() {
			let path = entry.path();
			if path.extension().map_or(false, |ext| ext == "c") {
				build.file(path);
			}
		}
	}
}

fn ensure_opus_source(opus_path: &path::Path) {
	use git2::{build::RepoBuilder, FetchOptions};

	// Check if opus directory exists and has content
	if opus_path.exists() && opus_path.join("include").exists() {
		println!("cargo:info=Using existing Opus source at {:?}", opus_path);
		return;
	}

	println!("cargo:warning=Cloning Opus repository with shallow depth...");

	// Configure shallow clone with depth=1 for faster cloning
	let mut fetch_options = FetchOptions::new();
	fetch_options.depth(1);

	let mut builder = RepoBuilder::new();
	builder.fetch_options(fetch_options);

	// Clone the repository with shallow depth (main branch first)
	let repo = builder
		.clone("https://github.com/xiph/opus.git", opus_path)
		.expect("Failed to clone Opus repository");

	// Checkout the specified Opus tag
	let obj = repo
		.revparse_single(OPUS_TAG_VERSION)
		.unwrap_or_else(|_| panic!("Failed to find {} tag", OPUS_TAG_VERSION));
	repo.checkout_tree(&obj, None)
		.unwrap_or_else(|_| panic!("Failed to checkout {}", OPUS_TAG_VERSION));
	repo.set_head_detached(obj.id())
		.unwrap_or_else(|_| panic!("Failed to set HEAD to {}", OPUS_TAG_VERSION));

	println!("cargo:warning=Opus {} cloned successfully (shallow)", OPUS_TAG_VERSION);
}
