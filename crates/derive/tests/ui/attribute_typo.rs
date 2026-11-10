use sarthi_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
enum Event {
	Grid(#[tagged_enum(flaten)] Vec<u32>),
	Other { method: String, value: rmpv::Value },
}

fn main() {}
