use sarthi_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
enum Event {
	Grid(#[tagged_enum(flatten)] Option<u32>),
	Other { method: String, value: rmpv::Value },
}

fn main() {}
