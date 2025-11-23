use crate::data::preimage_repository::{PreimageMetadata, PreimageRepository};
use axum::async_trait;
use std::sync::{Arc, RwLock};
use tokio::fs;
use tracing::{error, info};

#[derive(Clone)]
pub struct FilePreimageRepository {
    dir: String,
    metadata_list: Arc<RwLock<Vec<PreimageMetadata>>>,
}

impl FilePreimageRepository {
    pub async fn new(parent_dir: &str) -> anyhow::Result<Self> {
        let metadata_list = Self::load_metadata(parent_dir).await?;
        info!("loaded metadata: {:?}", metadata_list.len());
        Ok(Self {
            dir: parent_dir.to_string(),
            metadata_list: Arc::new(RwLock::new(metadata_list)),
        })
    }

    async fn load_metadata(dir: &str) -> anyhow::Result<Vec<PreimageMetadata>> {
        let mut metadata_list = vec![];

        let mut entries = fs::read_dir(dir)
            .await
            .map_err(|e| anyhow::anyhow!("failed to read dir: {dir:?}, error={e}"))?;
        while let Some(entry) = entries.next_entry().await? {
            match entry.file_name().to_str() {
                None => continue,
                Some(name) => {
                    let metadata = PreimageMetadata::try_from(name);
                    match metadata {
                        Err(e) => {
                            error!("failed to parse metadata: {:?}, error={}", name, e);
                        }
                        Ok(_) => metadata_list.push(metadata.unwrap()),
                    }
                }
            }
        }
        Self::sort(&mut metadata_list);
        Ok(metadata_list)
    }

    fn path(&self, metadata: &PreimageMetadata) -> String {
        format!(
            "{}/{}_{}_{}",
            self.dir, metadata.agreed, metadata.claimed, metadata.l1_head
        )
    }
    fn sort(metadata_list: &mut [PreimageMetadata]) {
        metadata_list.sort_by(|a, b| a.claimed.cmp(&b.claimed));
    }
}

#[async_trait]
impl PreimageRepository for FilePreimageRepository {
    async fn upsert(&self, metadata: PreimageMetadata, preimage: Vec<u8>) -> anyhow::Result<()> {
        let path = self.path(&metadata);
        fs::write(&path, preimage).await?;
        // NOTE: Process should be restarted when locking is failed to avoid dirty metadata.
        let mut lock = self.metadata_list.write().unwrap();
        lock.push(metadata);
        Ok(())
    }
    async fn get(&self, metadata: &PreimageMetadata) -> anyhow::Result<Vec<u8>> {
        let path = self.path(metadata);
        let preimage = fs::read(&path).await?;
        Ok(preimage)
    }
    async fn list_metadata(&self, gt_claimed: Option<u64>) -> Vec<PreimageMetadata> {
        let mut raw = {
            let lock = self.metadata_list.read().unwrap();
            lock.clone()
        };
        Self::sort(&mut raw);
        match gt_claimed {
            None => raw,
            Some(gt_claimed) => raw.into_iter().filter(|m| m.claimed > gt_claimed).collect(),
        }
    }

    async fn latest_metadata(&self) -> Option<PreimageMetadata> {
        let mut result = self.list_metadata(None).await;
        result.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::FilePreimageRepository;
    use crate::data::preimage_repository::{PreimageMetadata, PreimageRepository};
    use alloy_primitives::B256;
    use std::path::PathBuf;

    fn unique_test_dir(suffix: &str) -> String {
        let mut dir = std::env::temp_dir();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pid = std::process::id();
        dir.push(format!("optimism_preimage_maker_test_{pid}_{ts}_{suffix}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir.to_string_lossy().to_string()
    }

    fn make_meta(agreed: u64, claimed: u64, head: B256) -> PreimageMetadata {
        PreimageMetadata {
            agreed,
            claimed,
            l1_head: head,
        }
    }

    #[tokio::test]
    async fn test_new_empty_dir() {
        let dir = unique_test_dir("empty");
        let repo = FilePreimageRepository::new(&dir).await.expect("repo new");
        let list = repo.list_metadata(None).await;
        assert!(list.is_empty());
        let latest = repo.latest_metadata().await;
        assert!(latest.is_none());
        tokio::fs::remove_dir_all(dir).await.ok();
    }

    #[tokio::test]
    async fn test_upsert_and_get_and_latest() {
        let dir = unique_test_dir("upsert");
        let repo = FilePreimageRepository::new(&dir).await.expect("repo new");

        let h = B256::from([1u8; 32]);
        let m1 = make_meta(1, 2, h);
        let m2 = make_meta(2, 3, h);

        let p1 = b"hello".to_vec();
        let p2 = b"world".to_vec();

        repo.upsert(m1.clone(), p1.clone())
            .await
            .expect("upsert m1");
        repo.upsert(m2.clone(), p2.clone())
            .await
            .expect("upsert m2");

        let got1 = repo.get(&m1).await.expect("get m1");
        assert_eq!(got1, p1);
        let got2 = repo.get(&m2).await.expect("get m2");
        assert_eq!(got2, p2);

        // list should be sorted by agreed ascending
        let list = repo.list_metadata(None).await;
        assert_eq!(list, vec![m1.clone(), m2.clone()]);

        // latest should be the last after sorting (m2)
        let latest = repo.latest_metadata().await;
        assert_eq!(latest, Some(m2.clone()));

        tokio::fs::remove_dir_all(dir).await.ok();
    }

    #[tokio::test]
    async fn test_list_metadata_filter_and_sort() {
        let dir = unique_test_dir("filter");
        let repo = FilePreimageRepository::new(&dir).await.expect("repo new");
        let h = B256::from([2u8; 32]);
        let m_a = make_meta(5, 6, h);
        let m_b = make_meta(1, 2, h);
        let m_c = make_meta(3, 4, h);

        repo.upsert(m_a.clone(), vec![10]).await.unwrap();
        repo.upsert(m_b.clone(), vec![20]).await.unwrap();
        repo.upsert(m_c.clone(), vec![30]).await.unwrap();

        let all = repo.list_metadata(None).await;
        assert_eq!(all, vec![m_b.clone(), m_c.clone(), m_a.clone()]);

        // filter by claimed > x
        let filtered = repo.list_metadata(Some(2)).await; // claimed > 2 => m_c(4), m_a(6)
        assert_eq!(filtered, vec![m_c.clone(), m_a.clone()]);

        tokio::fs::remove_dir_all(dir).await.ok();
    }

    #[tokio::test]
    async fn test_load_existing_files() {
        let dir = unique_test_dir("load");

        // Pre-create files with valid names and contents
        let h1 = B256::from([3u8; 32]);
        let h2 = B256::from([4u8; 32]);
        let m1 = make_meta(10, 11, h1);
        let m2 = make_meta(12, 13, h2);

        let f1 = format!("{}_{}_{}", m1.agreed, m1.claimed, m1.l1_head);
        let f2 = format!("{}_{}_{}", m2.agreed, m2.claimed, m2.l1_head);
        let mut p1 = PathBuf::from(&dir);
        p1.push(&f1);
        let mut p2 = PathBuf::from(&dir);
        p2.push(&f2);

        tokio::fs::write(&p1, b"alpha").await.unwrap();
        tokio::fs::write(&p2, b"beta").await.unwrap();
        // invalid file should be ignored
        let mut junk = PathBuf::from(&dir);
        junk.push("invalid_name.txt");
        tokio::fs::write(&junk, b"junk").await.unwrap();

        let repo = FilePreimageRepository::new(&dir).await.expect("repo new");

        let list = repo.list_metadata(None).await;
        // Should be sorted by agreed
        assert_eq!(list, vec![m1.clone(), m2.clone()]);

        let g1 = repo.get(&m1).await.unwrap();
        assert_eq!(g1, b"alpha");
        let g2 = repo.get(&m2).await.unwrap();
        assert_eq!(g2, b"beta");

        tokio::fs::remove_dir_all(dir).await.ok();
    }

    #[tokio::test]
    async fn test_overwrite_same_metadata_replaces_file() {
        let dir = unique_test_dir("overwrite");
        let repo = FilePreimageRepository::new(&dir).await.expect("repo new");
        let h = B256::from([5u8; 32]);
        let m = make_meta(7, 8, h);

        repo.upsert(m.clone(), b"v1".to_vec()).await.unwrap();
        let got = repo.get(&m).await.unwrap();
        assert_eq!(got, b"v1".to_vec());

        // overwrite
        repo.upsert(m.clone(), b"v2".to_vec()).await.unwrap();
        let got2 = repo.get(&m).await.unwrap();
        assert_eq!(got2, b"v2".to_vec());

        tokio::fs::remove_dir_all(dir).await.ok();
    }
}
