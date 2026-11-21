#[cfg(test)]
mod tests {
    use crate::apis::{Downloaded, ImageTask};
    use uuid::Uuid;

    #[tokio::test]
    async fn test_image_task_state_machine() {
        // Create a fake 1x1 png image
        let img_bytes = vec![
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 2, 0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 8, 215, 99, 248, 255,
            255, 63, 0, 5, 254, 2, 254, 220, 204, 89, 231, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96,
            130,
        ];

        let listing_id = Uuid::new_v4();
        let image_id = Uuid::new_v4();

        // 1. We mock the Downloaded state directly because download() relies on the GCP client
        let downloaded = ImageTask {
            listing_id,
            image_id,
            state: Downloaded(img_bytes),
        };

        // 2. Transition Downloaded -> Processed
        let processed = downloaded.process().await.expect("Failed to process image");

        // 3. Verify the Processed state has 5 variants
        assert_eq!(processed.state.0.len(), 5);
        let variants = &processed.state.0;

        // Thumbnail
        assert_eq!(variants[0].0, 400);
        assert_eq!(variants[0].1, "thumbnail");

        // Mobile
        assert_eq!(variants[1].0, 720);
        assert_eq!(variants[1].1, "mobile");

        // Tablet
        assert_eq!(variants[2].0, 1280);
        assert_eq!(variants[2].1, "tablet");
    }

    #[tokio::test]
    async fn test_process_handles_concurrent_burst() {
        let img_bytes = vec![
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 2, 0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 8, 215, 99, 248, 255,
            255, 63, 0, 5, 254, 2, 254, 220, 204, 89, 231, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96,
            130,
        ];

        let mut handles = Vec::new();
        for _ in 0..6 {
            let downloaded = ImageTask {
                listing_id: Uuid::new_v4(),
                image_id: Uuid::new_v4(),
                state: Downloaded(img_bytes.clone()),
            };
            handles.push(tokio::spawn(async move {
                let processed = downloaded.process().await.map_err(|e| e.to_string())?;
                Ok::<usize, String>(processed.state.0.len())
            }));
        }

        for handle in handles {
            let count = handle
                .await
                .expect("task should not panic")
                .expect("process should succeed");
            assert_eq!(count, 5);
        }
    }
}
