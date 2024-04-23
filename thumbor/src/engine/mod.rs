mod photon;

use crate::pb::Spec;
use image::ImageFormat;
pub use photon::Photon;

// Engine trait 未来可以添加更多的 engine，主流程只需要替换 engine
pub trait Engine {
    // 对 engine 按照specs 进行一些列有序的处理
    fn apple(&mut self, specs: &[Spec]);

    // 从 engine 中生成目标图片，注意这里用的是self 而非 self的引用
    fn generate(self, format: ImageFormat) -> Vec<u8>;

}


// SpecTransform 未来如果添加跟多的 spec，只需要实现它即可
pub trait SpecTransform<T> {

    // 对图片使用 op 做 transform
    fn transform(&mut self, op:T);
}