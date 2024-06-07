use grok_rs::Grok;

fn main() {
    let grok = Grok::default();
    let pattern = grok
        // USERNAME and EMAILADDRESS are defined in grok-patterns
        .compile("%{USERNAME}", false)
        .unwrap();

    let result = pattern.parse("admin admin@example.com").unwrap();
    println!("{:#?}", result);
}
