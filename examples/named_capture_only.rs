use grok_rs::Grok;

fn main() {
    let grok = Grok::default();
    let pattern = grok
        .compile("%{USERNAME} %{EMAILADDRESS:email}", true)
        .unwrap();
    let result = pattern.parse("admin admin@example.com").unwrap();
    println!("{:#?}", result);
}
