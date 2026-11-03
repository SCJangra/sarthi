use nvui_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
enum Event {
	Other { method: u32, value: rmpv::Value },
}

fn main() {}
