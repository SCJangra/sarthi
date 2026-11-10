use sarthi_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
enum Event {
	Grid(#[tagged_enum(flatten)] Vec<u32>, String),
	Other { method: String, value: rmpv::Value },
}

fn main() {}
