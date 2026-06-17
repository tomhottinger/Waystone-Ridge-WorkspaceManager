fn main() {
    // Application-Manifest einbetten (Common Controls v6, DPI-Awareness).
    embed_resource::compile("app.rc", embed_resource::NONE);
}
