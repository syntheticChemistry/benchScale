fn main() {
    // Link against libvirt when libvirt feature is enabled
    #[cfg(feature = "libvirt")]
    {
        println!("cargo:rustc-link-lib=virt");
    }
}

