use sarthi_derive::DeserializeTaggedEnum;

#[allow(non_camel_case_types)]
#[derive(DeserializeTaggedEnum)]
enum Event {
	GridLine,
	Grid_Line,
	Other { method: String, value: rmpv::Value },
}

fn main() {}
