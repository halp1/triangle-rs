macro_rules! event {
	($($path:ident).+ => $struct_name:ident = $original:path) => {
		pub type $struct_name = $original;
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

#[macro_export]
macro_rules! partial {
	(
		$name:ident {
			$(
				$field:ident : $ty:ty
			),* $(,)?
		}
	) => {
		#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]

		pub struct $name {
			$(pub $field: Option<$ty>),*
		}

		impl crate::utils::Partial for $name {
			type Target = $name;

			fn merge(self, other: Self) -> Self {
				Self {
					$(
						$field: other.$field.or(self.$field),
					)*
				}
			}

			fn apply(self, target: &mut Self::Target) {
				$(
					if let Some(value) = self.$field {
						target.$field.replace(value);
					}
				)*
			}
		}
	};
}

pub(crate) use partial;