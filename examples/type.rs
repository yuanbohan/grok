use grok_rs::Grok;

fn main() {
    let mut grok = Grok::default();
    grok.add_pattern("NUMBER", r"\d+");

    let pattern = grok.compile("%{NUMBER:digit:int}", false).unwrap();
    let result = pattern.parse("hello 123").unwrap();
    println!("{:#?}", result);
}
