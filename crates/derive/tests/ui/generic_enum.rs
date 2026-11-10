use sarthi_derive::DeserializeTaggedEnum;

#[derive(DeserializeTaggedEnum)]
enum Event<T> {
	Grid(T),
	Other { method: String, value: rmpv::Value },
}

fn main() {}
