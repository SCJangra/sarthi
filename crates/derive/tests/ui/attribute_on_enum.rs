use nvui_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
#[tagged_enum(flatten)]
enum Event {
	Grid(Vec<u32>),
	Other { method: String, value: rmpv::Value },
}

fn main() {}
