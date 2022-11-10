use std::sync::Arc;

use async_trait::async_trait;
use http::{HeaderMap, HeaderValue};
use reqwest::{Response, Url};

use crate::{
    auth::VERB,
    bucket::Bucket,
    builder::{ArcPointer, RequestBuilder},
    config::ObjectBase,
    errors::{OssError, OssResult},
    object::ObjectList,
    types::{CanonicalizedResource, ContentRange},
    Client,
};
#[cfg(feature = "put_file")]
use infer::Infer;

/// # 文件相关功能
///
/// 包括 上传，下载，删除等功能
#[async_trait]
pub trait File: AlignBuilder {
    /// 根据文件路径获取最终的调用接口以及相关参数
    fn get_url(&self, key: &str) -> (Url, CanonicalizedResource);

    /// # 上传文件到 OSS
    ///
    /// 需指定文件的路径
    #[cfg(feature = "put_file")]
    async fn put_file<
        P: Into<std::path::PathBuf> + std::convert::AsRef<std::path::Path> + Send + Sync,
    >(
        &self,
        file_name: P,
        key: &str,
    ) -> OssResult<String> {
        let file_content = std::fs::read(file_name)?;

        let get_content_type = |content: &Vec<u8>| match Infer::new().get(content) {
            Some(con) => Some(con.mime_type()),
            None => None,
        };

        self.put_content(file_content, key, get_content_type).await
    }

    /// # 上传文件内容到 OSS
    ///
    /// 需指定要上传的文件内容
    /// 以及根据文件内容获取文件类型的闭包
    ///
    /// # Examples
    ///
    /// 上传 tauri 升级用的签名文件
    /// ```ignore
    /// # #[tokio::main]
    /// # async fn main(){
    /// use infer::Infer;
    /// # use dotenv::dotenv;
    /// # dotenv().ok();
    /// # let client = aliyun_oss_client::Client::from_env().unwrap();
    ///
    /// fn sig_match(buf: &[u8]) -> bool {
    ///     return buf.len() >= 3 && buf[0] == 0x64 && buf[1] == 0x57 && buf[2] == 0x35;
    /// }
    /// let mut infer = Infer::new();
    /// infer.add("application/pgp-signature", "sig", sig_match);
    ///
    /// let get_content_type = |content: &Vec<u8>| match infer.get(content) {
    ///     Some(con) => Some(con.mime_type()),
    ///     None => None,
    /// };
    /// let content: Vec<u8> = String::from("dW50cnVzdGVkIGNvbW1lbnQ6IHNpxxxxxxxxx").into_bytes();
    /// let res = client
    ///     .put_content(content, "xxxxxx.msi.zip.sig", get_content_type)
    ///     .await;
    /// assert!(res.is_ok());
    /// # }
    /// ```
    async fn put_content<F>(
        &self,
        content: Vec<u8>,
        key: &str,
        get_content_type: F,
    ) -> OssResult<String>
    where
        F: Fn(&Vec<u8>) -> Option<&'static str> + Send + Sync,
    {
        let content_type =
            get_content_type(&content).ok_or(OssError::Input("file type is known".to_string()))?;

        let content = self.put_content_base(content, content_type, key).await?;

        let result = content
            .headers()
            .get("ETag")
            .ok_or(OssError::Input("get Etag error".to_string()))?
            .to_str()
            .map_err(OssError::from)?;

        Ok(result.to_string())
    }

    /// 最核心的上传文件到 OSS 的方法
    async fn put_content_base(
        &self,
        content: Vec<u8>,
        content_type: &str,
        key: &str,
    ) -> OssResult<Response> {
        let (url, canonicalized) = self.get_url(key);

        let mut headers = HeaderMap::new();
        let content_length = content.len().to_string();
        headers.insert(
            "Content-Length",
            HeaderValue::from_str(&content_length).map_err(OssError::from)?,
        );

        headers.insert(
            "Content-Type",
            content_type.parse().map_err(OssError::from)?,
        );

        self.builder_with_header(VERB::PUT, url, canonicalized, Some(headers))?
            .body(content)
            .send()
            .await
    }

    /// # 获取 OSS 上的文件内容
    async fn get_object<R: Into<ContentRange> + Send + Sync>(
        &self,
        key: &str,
        range: R,
    ) -> OssResult<Vec<u8>> {
        let (url, canonicalized) = self.get_url(key);

        let headers = {
            let mut headers = HeaderMap::new();
            headers.insert("Range", range.into().into());
            headers
        };

        let content = self
            .builder_with_header("GET", url, canonicalized, Some(headers))?
            .send()
            .await?
            .text()
            .await?;

        Ok(content.into_bytes())
    }

    /// # 删除 OSS 上的文件
    async fn delete_object(&self, key: &str) -> OssResult<()> {
        let (url, canonicalized) = self.get_url(key);

        self.builder(VERB::DELETE, url, canonicalized)?
            .send()
            .await?;

        Ok(())
    }
}

impl File for Client {
    fn get_url(&self, key: &str) -> (Url, CanonicalizedResource) {
        let mut url = self.get_bucket_url();
        url.set_path(key);

        let object_base =
            ObjectBase::<ArcPointer>::new(Arc::new(self.get_bucket_base()), key.to_owned());

        let canonicalized = CanonicalizedResource::from_object(object_base, None);

        (url, canonicalized)
    }
}

impl File for Bucket {
    fn get_url(&self, key: &str) -> (Url, CanonicalizedResource) {
        let mut url = self.base.to_url();
        url.set_path(key);

        let object_base =
            ObjectBase::<ArcPointer>::new(Arc::new(self.base.clone()), key.to_owned());

        let canonicalized = CanonicalizedResource::from_object(object_base, None);

        (url, canonicalized)
    }
}

impl File for ObjectList<ArcPointer> {
    fn get_url(&self, key: &str) -> (Url, CanonicalizedResource) {
        let mut url = self.bucket.to_url();
        url.set_path(key);

        let object_base =
            ObjectBase::<ArcPointer>::new(Arc::new(self.bucket.clone()), key.to_owned());

        let canonicalized = CanonicalizedResource::from_object(object_base, None);

        (url, canonicalized)
    }
}

// TODO 未来可让 Object 结构体支持 上传，下载和删除
// #[async_trait]
// impl Object<ArcPointer> {
//     async fn put_content_base(
//         &self,
//         client: Arc<Client>,
//     ) -> OssResult<Response> {

//     }
// }

/// # 对齐 Client, Bucket, ObjectList 等结构体的 trait
///
/// 用于他们方便的实现 [`File`](./trait.File.html) trait
pub trait AlignBuilder: Send + Sync {
    #[inline]
    fn builder<M: Into<VERB>>(
        &self,
        method: M,
        url: Url,
        resource: CanonicalizedResource,
    ) -> OssResult<RequestBuilder> {
        self.builder_with_header(method, url, resource, None)
    }

    fn builder_with_header<M: Into<VERB>>(
        &self,
        method: M,
        url: Url,
        resource: CanonicalizedResource,
        headers: Option<HeaderMap>,
    ) -> OssResult<RequestBuilder>;
}

impl AlignBuilder for Bucket {
    fn builder_with_header<M: Into<VERB>>(
        &self,
        method: M,
        url: Url,
        resource: CanonicalizedResource,
        headers: Option<HeaderMap>,
    ) -> OssResult<RequestBuilder> {
        self.client()
            .builder_with_header(method, url, resource, headers)
    }
}

impl AlignBuilder for ObjectList<ArcPointer> {
    fn builder_with_header<M: Into<VERB>>(
        &self,
        method: M,
        url: Url,
        resource: CanonicalizedResource,
        headers: Option<HeaderMap>,
    ) -> OssResult<RequestBuilder> {
        self.client()
            .builder_with_header(method, url, resource, headers)
    }
}

#[cfg(feature = "blocking")]
pub use blocking::File as BlockingFile;

#[cfg(feature = "blocking")]
pub mod blocking {
    use std::rc::Rc;

    use crate::{
        auth::VERB,
        blocking::builder::RequestBuilder,
        bucket::Bucket,
        builder::RcPointer,
        config::ObjectBase,
        errors::{OssError, OssResult},
        object::ObjectList,
        types::{CanonicalizedResource, ContentRange},
        ClientRc,
    };
    use http::{HeaderMap, HeaderValue};
    #[cfg(feature = "put_file")]
    use infer::Infer;
    use reqwest::{blocking::Response, Url};

    pub trait File: AlignBuilder {
        /// 根据文件路径获取最终的调用接口以及相关参数
        fn get_url(&self, key: &str) -> (Url, CanonicalizedResource);

        /// # 上传文件到 OSS
        ///
        /// 需指定文件的路径
        #[cfg(feature = "put_file")]
        fn put_file<P: Into<std::path::PathBuf> + std::convert::AsRef<std::path::Path>>(
            &self,
            file_name: P,
            key: &'static str,
        ) -> OssResult<String> {
            let file_content = std::fs::read(file_name)?;

            let get_content_type = |content: &Vec<u8>| match Infer::new().get(content) {
                Some(con) => Some(con.mime_type()),
                None => None,
            };

            self.put_content(file_content, key, get_content_type)
        }

        /// # 上传文件内容到 OSS
        ///
        /// 需指定要上传的文件内容
        /// 以及根据文件内容获取文件类型的闭包
        ///
        /// # Examples
        ///
        /// 上传 tauri 升级用的签名文件
        /// ```ignore
        /// # fn main(){
        /// use infer::Infer;
        /// # use dotenv::dotenv;
        /// # dotenv().ok();
        /// # let client = aliyun_oss_client::ClientRc::from_env().unwrap();
        /// use crate::aliyun_oss_client::file::BlockingFile;
        ///
        /// fn sig_match(buf: &[u8]) -> bool {
        ///     return buf.len() >= 3 && buf[0] == 0x64 && buf[1] == 0x57 && buf[2] == 0x35;
        /// }
        /// let mut infer = Infer::new();
        /// infer.add("application/pgp-signature", "sig", sig_match);
        ///
        /// let get_content_type = |content: &Vec<u8>| match infer.get(content) {
        ///     Some(con) => Some(con.mime_type()),
        ///     None => None,
        /// };
        /// let content: Vec<u8> = String::from("dW50cnVzdGVkIGNvbW1lbnQ6IHNpxxxxxxxxx").into_bytes();
        /// let res = client
        ///     .put_content(content, "xxxxxx.msi.zip.sig", get_content_type);
        /// assert!(res.is_ok());
        /// # }
        /// ```
        fn put_content<F>(
            &self,
            content: Vec<u8>,
            key: &str,
            get_content_type: F,
        ) -> OssResult<String>
        where
            F: Fn(&Vec<u8>) -> Option<&'static str>,
        {
            let content_type = get_content_type(&content)
                .ok_or(OssError::Input("file type is known".to_string()))?;

            let content = self.put_content_base(content, content_type, key)?;

            let result = content
                .headers()
                .get("ETag")
                .ok_or(OssError::Input("get Etag error".to_string()))?
                .to_str()
                .map_err(OssError::from)?;

            Ok(result.to_string())
        }

        /// 最原始的上传文件的方法
        fn put_content_base(
            &self,
            content: Vec<u8>,
            content_type: &str,
            key: &str,
        ) -> OssResult<Response> {
            let (url, canonicalized) = self.get_url(key);

            let mut headers = HeaderMap::new();
            let content_length = content.len().to_string();
            headers.insert(
                "Content-Length",
                HeaderValue::from_str(&content_length).map_err(OssError::from)?,
            );

            headers.insert(
                "Content-Type",
                content_type.parse().map_err(OssError::from)?,
            );

            let response = self
                .builder_with_header(VERB::PUT, url, canonicalized, Some(headers))?
                .body(content);

            let content = response.send()?;
            Ok(content)
        }

        /// # 获取文件内容
        fn get_object<R: Into<ContentRange>>(&self, key: &str, range: R) -> OssResult<Vec<u8>> {
            let (url, canonicalized) = self.get_url(key);

            let headers = {
                let mut headers = HeaderMap::new();
                headers.insert("Range", range.into().into());
                headers
            };

            Ok(self
                .builder_with_header("GET", url, canonicalized, Some(headers))?
                .send()?
                .text()?
                .into_bytes())
        }

        fn delete_object(&self, key: &str) -> OssResult<()> {
            let (url, canonicalized) = self.get_url(key);

            self.builder(VERB::DELETE, url, canonicalized)?.send()?;

            Ok(())
        }
    }

    impl File for ClientRc {
        fn get_url(&self, key: &str) -> (Url, CanonicalizedResource) {
            let mut url = self.get_bucket_url();
            url.set_path(key);

            let object_base =
                ObjectBase::<RcPointer>::new(Rc::new(self.get_bucket_base()), key.to_owned());

            let canonicalized = CanonicalizedResource::from_object(object_base, None);

            (url, canonicalized)
        }
    }

    impl File for Bucket<RcPointer> {
        fn get_url(&self, key: &str) -> (Url, CanonicalizedResource) {
            let mut url = self.base.to_url();
            url.set_path(key);

            let object_base =
                ObjectBase::<RcPointer>::new(Rc::new(self.base.clone()), key.to_owned());

            let canonicalized = CanonicalizedResource::from_object(object_base, None);

            (url, canonicalized)
        }
    }

    impl File for ObjectList<RcPointer> {
        fn get_url(&self, key: &str) -> (Url, CanonicalizedResource) {
            let mut url = self.bucket.to_url();
            url.set_path(key);

            let object_base =
                ObjectBase::<RcPointer>::new(Rc::new(self.bucket.clone()), key.to_owned());

            let canonicalized = CanonicalizedResource::from_object(object_base, None);

            (url, canonicalized)
        }
    }

    pub trait AlignBuilder {
        #[inline]
        fn builder<M: Into<VERB>>(
            &self,
            method: M,
            url: Url,
            resource: CanonicalizedResource,
        ) -> OssResult<RequestBuilder> {
            self.builder_with_header(method, url, resource, None)
        }

        fn builder_with_header<M: Into<VERB>>(
            &self,
            method: M,
            url: Url,
            resource: CanonicalizedResource,
            headers: Option<HeaderMap>,
        ) -> OssResult<RequestBuilder>;
    }

    /// # 对齐 Client, Bucket, ObjectList 等结构体的 trait
    ///
    /// 用于他们方便的实现 [`File`](./trait.File.html) trait
    impl AlignBuilder for Bucket<RcPointer> {
        fn builder_with_header<M: Into<VERB>>(
            &self,
            method: M,
            url: Url,
            resource: CanonicalizedResource,
            headers: Option<HeaderMap>,
        ) -> OssResult<RequestBuilder> {
            self.client()
                .builder_with_header(method, url, resource, headers)
        }
    }

    impl AlignBuilder for ObjectList<RcPointer> {
        fn builder_with_header<M: Into<VERB>>(
            &self,
            method: M,
            url: Url,
            resource: CanonicalizedResource,
            headers: Option<HeaderMap>,
        ) -> OssResult<RequestBuilder> {
            self.client()
                .builder_with_header(method, url, resource, headers)
        }
    }
}
