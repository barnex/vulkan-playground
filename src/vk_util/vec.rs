use vulkano::image::Dimensions;

#[derive(Copy, Clone, Debug)]
pub struct UVec2(u32, u32);

impl From<(u32, u32)> for UVec2 {
	fn from(v: (u32, u32)) -> Self {
		Self(v.0, v.1)
	}
}

impl Into<Dimensions> for UVec2 {
	fn into(self) -> Dimensions {
		Dimensions::Dim2d {
			width: self.0,
			height: self.1,
		}
	}
}
