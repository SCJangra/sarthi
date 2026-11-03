use nvui_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
enum Event {
	Other { method: String, value: rmpv::Value, extra: u32 },
}

fn main() {}
