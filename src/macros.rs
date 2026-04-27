macro_rules! event {
	($($path:ident).+ => $struct_name:ident = $original:path) => {
		type $struct_name = $original;
		impl crate::utils::events::Event for $struct_name {
			const NAME: &'static str = stringify!($($path).+);
		}
	};

	($($path:ident).+ => $struct_name:ident) => {
		#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
		pub struct $struct_name;

		impl crate::utils::events::Event for $struct_name {
			const NAME: &'static str = stringify!($($path).+);
		}
	};

	($($path:ident).+ => $struct_name:ident ( $inner:ty )) => {
		#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
		pub struct $struct_name(pub $inner);

		impl crate::utils::events::Event for $struct_name {
			const NAME: &'static str = stringify!($($path).+);
		}
};

	($($path:ident).+ => $struct_name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
		#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
		pub struct $struct_name {
			$(pub $field: $ty),*
		}

		impl crate::utils::events::Event for $struct_name {
			const NAME: &'static str = stringify!($($path).+);
		}
	};
}

pub(crate) use event;