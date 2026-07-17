const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
    </windowsSettings>
  </application>
</assembly>
"#;

fn main() {
    let mut res = winresource::WindowsResource::new();
    res.set_icon("icons/GitWyrmIcon.ico");
    res.set("ProductName", "GitWyrm Setup");
    res.set("CompanyName", "GitWyrm");
    res.set("FileDescription", "GitWyrm Setup");
    res.set("LegalCopyright", "Copyright 2026 GitWyrm");
    res.set_manifest(MANIFEST);
    res.compile().expect("Failed to compile Windows resources");
}
