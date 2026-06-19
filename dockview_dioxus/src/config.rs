use v_utils::macros as v_macros;

#[derive(Clone, Debug, Default, v_macros::LiveSettings, v_macros::MyConfigPrimitives, v_macros::Settings)]
pub struct AppConfig {
	#[primitives(skip)]
	#[serde(default = "__default_example_greet")]
	pub example_greet: String,
}
fn __default_example_greet() -> String {
	"World".to_string()
}
