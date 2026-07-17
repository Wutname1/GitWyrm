fn main() {
    let mut res = winresource::WindowsResource::new();
    res.set_icon("icons/GitWyrmIcon.ico");
    res.set("ProductName", "GitWyrm Setup");
    res.set("CompanyName", "GitWyrm");
    res.set("FileDescription", "GitWyrm Setup");
    res.set("LegalCopyright", "Copyright 2026 GitWyrm");
    res.compile().expect("Failed to compile Windows resources");
}
