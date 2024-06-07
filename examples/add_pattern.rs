use grok_rs::Grok;

fn main() {
    let mut grok = Grok::default();
    grok.add_pattern("NAME", r"[A-z0-9._-]+");
    let pattern = grok.compile("%{NAME}", false).unwrap();
    let result = pattern.parse("admin").unwrap();
    println!("{:#?}", result);
}
