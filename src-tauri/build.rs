fn main() {
    // Embute o manifest que exige privilégios de administrador (requireAdministrator).
    let windows =
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("wintune.exe.manifest"));
    let attributes = tauri_build::Attributes::new().windows_attributes(windows);

    if let Err(erro) = tauri_build::try_build(attributes) {
        panic!("falha ao executar o build do Tauri: {erro:#}");
    }
}
