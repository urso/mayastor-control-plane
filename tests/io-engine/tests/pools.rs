use deployer_cluster::ClusterBuilder;
use grpc::operations::pool::traits::PoolOperations;
use openapi::models;
use stor_port::types::v0::{store::pool::RaidConfig as StoreRaidConfig, transport as v0};

#[tokio::test]
async fn create_pool_malloc() {
    let cluster = ClusterBuilder::builder().build().await.unwrap();
    cluster
        .rest_v00()
        .pools_api()
        .put_node_pool(
            cluster.node(0).as_str(),
            cluster.pool(0, 0).as_str(),
            models::CreatePoolBody::new(vec!["malloc:///disk?size_mb=100"]),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn create_pool_with_missing_disk() {
    let cluster = ClusterBuilder::builder().build().await.unwrap();

    cluster
        .rest_v00()
        .pools_api()
        .put_node_pool(
            cluster.node(0).as_str(),
            cluster.pool(0, 0).as_str(),
            models::CreatePoolBody::new(vec!["/dev/c/3po"]),
        )
        .await
        .expect_err("Device should not exist");
}

#[tokio::test]
async fn create_pool_with_existing_disk() {
    let cluster = ClusterBuilder::builder().build().await.unwrap();

    cluster
        .rest_v00()
        .pools_api()
        .put_node_pool(
            cluster.node(0).as_str(),
            cluster.pool(0, 0).as_str(),
            models::CreatePoolBody::new(vec!["malloc:///disk?size_mb=100"]),
        )
        .await
        .unwrap();

    cluster
        .rest_v00()
        .pools_api()
        .put_node_pool(
            cluster.node(0).as_str(),
            cluster.pool(0, 0).as_str(),
            models::CreatePoolBody::new(vec!["malloc:///disk?size_mb=100"]),
        )
        .await
        .expect_err("Disk should be used by another pool");

    cluster
        .rest_v00()
        .pools_api()
        .del_pool(cluster.pool(0, 0).as_str())
        .await
        .unwrap();

    cluster
        .rest_v00()
        .pools_api()
        .put_node_pool(
            cluster.node(0).as_str(),
            cluster.pool(0, 0).as_str(),
            models::CreatePoolBody::new(vec!["malloc:///disk?size_mb=100"]),
        )
        .await
        .expect("Should now be able to create the new pool");
}

#[tokio::test]
async fn create_pool_idempotent() {
    let cluster = ClusterBuilder::builder().build().await.unwrap();
    let pool_client = cluster.grpc_client().pool();

    pool_client
        .create(
            &v0::CreatePool {
                node: cluster.node(0),
                id: cluster.pool(0, 0),
                disks: vec!["malloc:///disk?size_mb=100".into()],
                labels: None,
                encryption: None,
                cluster_size: None,
                raid_config: None,
            },
            None,
        )
        .await
        .unwrap();

    pool_client
        .create(
            &v0::CreatePool {
                node: cluster.node(0),
                id: cluster.pool(0, 0),
                disks: vec!["malloc:///disk?size_mb=100".into()],
                labels: None,
                encryption: None,
                cluster_size: None,
                raid_config: None,
            },
            None,
        )
        .await
        .expect_err("already exists");
}

/// FIXME: CAS-710
#[tokio::test]
async fn create_pool_idempotent_same_disk_different_query() {
    let cluster = ClusterBuilder::builder().build().await.unwrap();
    let pool_client = cluster.grpc_client().pool();
    pool_client
        .create(
            &v0::CreatePool {
                node: cluster.node(0),
                id: cluster.pool(0, 0),
                disks: vec!["malloc:///disk?size_mb=100&blk_size=512".into()],
                labels: None,
                encryption: None,
                cluster_size: None,
                raid_config: None,
            },
            None,
        )
        .await
        .unwrap();

    pool_client
        .create(
            &v0::CreatePool {
                node: cluster.node(0),
                id: cluster.pool(0, 0),
                disks: vec!["malloc:///disk?size_mb=200&blk_size=4096".into()],
                labels: None,
                encryption: None,
                cluster_size: None,
                raid_config: None,
            },
            None,
        )
        .await
        .expect_err("Different query not allowed!");
}

#[tokio::test]
async fn create_pool_idempotent_different_nvmf_host() {
    let cluster = ClusterBuilder::builder()
        .with_options(|opts| opts.with_io_engines(3))
        .build()
        .await
        .unwrap();
    let pool_client = cluster.grpc_client().pool();
    pool_client
        .create(
            &v0::CreatePool {
                node: cluster.node(1),
                id: cluster.pool(1, 0),
                disks: vec!["malloc:///disk?size_mb=100".into()],
                labels: None,
                encryption: None,
                cluster_size: None,
                raid_config: None,
            },
            None,
        )
        .await
        .unwrap();

    pool_client
        .create(
            &v0::CreatePool {
                node: cluster.node(2),
                id: cluster.pool(2, 0),
                disks: vec!["malloc:///disk?size_mb=100".into()],
                labels: None,
                encryption: None,
                cluster_size: None,
                raid_config: None,
            },
            None,
        )
        .await
        .unwrap();

    pool_client
        .create(
            &v0::CreatePool {
                node: cluster.node(2),
                id: cluster.pool(2, 0),
                disks: vec!["malloc:///disk?size_mb=100".into()],
                labels: None,
                encryption: None,
                cluster_size: None,
                raid_config: None,
            },
            None,
        )
        .await
        .expect_err("Pool Already exists!");

    pool_client
        .create(
            &v0::CreatePool {
                node: cluster.node(2),
                id: cluster.pool(2, 0),
                disks: vec!["malloc:///disk?size_mb=100".into()],
                labels: None,
                encryption: None,
                cluster_size: None,
                raid_config: None,
            },
            None,
        )
        .await
        .expect_err("Pool disk already used by another pool!");
}

#[tokio::test]
async fn create_raid0_pool_via_rest() {
    let cluster = ClusterBuilder::builder().build().await.unwrap();

    let raid_config = models::RaidConfig::raid0(models::Raid0Config::new(64u32));
    let mut pool_body = models::CreatePoolBody::new(vec![
        "malloc:///disk0?size_mb=64",
        "malloc:///disk1?size_mb=64",
    ]);
    pool_body.raid_config = Some(raid_config);

    cluster
        .rest_v00()
        .pools_api()
        .put_node_pool(cluster.node(0).as_str(), "raid0-pool", pool_body)
        .await
        .unwrap();

    // Validate pool properties
    let pool = cluster
        .rest_v00()
        .pools_api()
        .get_pool("raid0-pool")
        .await
        .unwrap();

    // RAID0 pool capacity should be less than sum of devices due to metadata overhead
    let state = pool.state.expect("Pool should have state");
    // With 2x64MB devices, expect capacity < 128MB due to lvs metadata overhead
    // but should still have meaningful capacity > 100MB
    assert!(state.capacity > 100 * 1024 * 1024 && state.capacity < 128 * 1024 * 1024);
    assert_eq!(state.status, models::PoolStatus::Online);
}

#[tokio::test]
async fn create_raid0_pool_via_grpc() {
    let cluster = ClusterBuilder::builder().build().await.unwrap();
    let pool_client = cluster.grpc_client().pool();

    let raid_config = Some(StoreRaidConfig::Raid0 { strip_size_kb: 64 });

    pool_client
        .create(
            &v0::CreatePool {
                node: cluster.node(0),
                id: "raid0-grpc-pool".into(),
                disks: vec![
                    "malloc:///disk0?size_mb=64".into(),
                    "malloc:///disk1?size_mb=64".into(),
                ],
                labels: None,
                encryption: None,
                cluster_size: None,
                raid_config,
            },
            None,
        )
        .await
        .unwrap();

    // Validate pool properties via gRPC
    let pools = pool_client.get(v0::Filter::None, None).await.unwrap();
    let raid_pool = pools
        .0
        .into_iter()
        .find(|p| p.id().to_string() == "raid0-grpc-pool")
        .expect("RAID0 pool should exist");

    // RAID0 pool capacity should be less than sum of devices due to metadata overhead
    let pool_state = raid_pool.state().expect("Pool should have state");
    // With 2x64MB devices, expect capacity < 128MB due to lvs metadata overhead
    // but should still have meaningful capacity > 100MB
    assert!(pool_state.capacity > 100 * 1024 * 1024 && pool_state.capacity < 128 * 1024 * 1024);
    assert_eq!(pool_state.status, v0::PoolStatus::Online);
}

#[tokio::test]
async fn raid0_pool_lifecycle() {
    let cluster = ClusterBuilder::builder().build().await.unwrap();

    // Create RAID0 pool
    let raid_config = models::RaidConfig::raid0(models::Raid0Config::new(64u32));
    let mut pool_body = models::CreatePoolBody::new(vec![
        "malloc:///disk0?size_mb=64",
        "malloc:///disk1?size_mb=64",
    ]);
    pool_body.raid_config = Some(raid_config);

    cluster
        .rest_v00()
        .pools_api()
        .put_node_pool(cluster.node(0).as_str(), "lifecycle-pool", pool_body)
        .await
        .unwrap();

    // Verify pool exists and is online
    let pool = cluster
        .rest_v00()
        .pools_api()
        .get_pool("lifecycle-pool")
        .await
        .unwrap();

    let state = pool.state.expect("Pool should have state");
    assert_eq!(state.status, models::PoolStatus::Online);
    // With 2x64MB devices, expect capacity < 128MB due to lvs metadata overhead
    // but should still have meaningful capacity > 100MB
    assert!(state.capacity > 100 * 1024 * 1024 && state.capacity < 128 * 1024 * 1024);

    // Delete pool
    cluster
        .rest_v00()
        .pools_api()
        .del_pool("lifecycle-pool")
        .await
        .unwrap();

    // Verify pool no longer exists
    cluster
        .rest_v00()
        .pools_api()
        .get_pool("lifecycle-pool")
        .await
        .expect_err("Pool should be deleted");
}

#[tokio::test]
async fn create_raid0_pool_idempotent() {
    let cluster = ClusterBuilder::builder().build().await.unwrap();
    let pool_client = cluster.grpc_client().pool();
    let raid_config = Some(StoreRaidConfig::Raid0 { strip_size_kb: 64 });

    pool_client
        .create(
            &v0::CreatePool {
                node: cluster.node(0),
                id: "raid0-idempotent-pool".into(),
                disks: vec![
                    "malloc:///disk0?size_mb=64".into(),
                    "malloc:///disk1?size_mb=64".into(),
                ],
                labels: None,
                encryption: None,
                cluster_size: None,
                raid_config: raid_config.clone(),
            },
            None,
        )
        .await
        .unwrap();

    pool_client
        .create(
            &v0::CreatePool {
                node: cluster.node(0),
                id: "raid0-idempotent-pool".into(),
                disks: vec![
                    "malloc:///disk0?size_mb=64".into(),
                    "malloc:///disk1?size_mb=64".into(),
                ],
                labels: None,
                encryption: None,
                cluster_size: None,
                raid_config,
            },
            None,
        )
        .await
        .expect_err("already exists");
}

#[tokio::test]
async fn create_raid0_pool_with_existing_disk() {
    let cluster = ClusterBuilder::builder().build().await.unwrap();

    // Create first RAID0 pool
    let raid_config = models::RaidConfig::raid0(models::Raid0Config::new(64u32));
    let mut pool_body = models::CreatePoolBody::new(vec![
        "malloc:///disk0?size_mb=64",
        "malloc:///disk1?size_mb=64",
    ]);
    pool_body.raid_config = Some(raid_config.clone());

    cluster
        .rest_v00()
        .pools_api()
        .put_node_pool(cluster.node(0).as_str(), "first-raid0-pool", pool_body)
        .await
        .unwrap();

    // Try to create second pool using same disk - should fail
    let mut conflicting_body = models::CreatePoolBody::new(vec![
        "malloc:///disk0?size_mb=64", // Same disk as first pool
        "malloc:///disk2?size_mb=64",
    ]);
    conflicting_body.raid_config = Some(raid_config.clone());

    cluster
        .rest_v00()
        .pools_api()
        .put_node_pool(
            cluster.node(0).as_str(),
            "second-raid0-pool",
            conflicting_body,
        )
        .await
        .expect_err("Disk should be used by another pool");

    // Clean up and verify disk can be reused after deletion
    cluster
        .rest_v00()
        .pools_api()
        .del_pool("first-raid0-pool")
        .await
        .unwrap();

    let mut reuse_body = models::CreatePoolBody::new(vec![
        "malloc:///disk0?size_mb=64",
        "malloc:///disk3?size_mb=64",
    ]);
    reuse_body.raid_config = Some(raid_config);

    cluster
        .rest_v00()
        .pools_api()
        .put_node_pool(cluster.node(0).as_str(), "reuse-raid0-pool", reuse_body)
        .await
        .expect("Should now be able to create the new pool");
}

#[tokio::test]
async fn raid0_pool_error_scenarios() {
    let cluster = ClusterBuilder::builder().build().await.unwrap();
    let pool_client = cluster.grpc_client().pool();

    // Test 1: Single disk with RAID0 config should fail
    let single_disk_raid = Some(StoreRaidConfig::Raid0 { strip_size_kb: 64 });
    pool_client
        .create(
            &v0::CreatePool {
                node: cluster.node(0),
                id: "single-disk-raid0".into(),
                disks: vec!["malloc:///disk0?size_mb=64".into()],
                labels: None,
                encryption: None,
                cluster_size: None,
                raid_config: single_disk_raid,
            },
            None,
        )
        .await
        .expect_err("Single disk RAID0 should fail");

    // Test 2: Invalid strip size (0) should fail
    let zero_strip = Some(StoreRaidConfig::Raid0 { strip_size_kb: 0 });
    pool_client
        .create(
            &v0::CreatePool {
                node: cluster.node(0),
                id: "zero-strip-raid0".into(),
                disks: vec![
                    "malloc:///disk1?size_mb=64".into(),
                    "malloc:///disk2?size_mb=64".into(),
                ],
                labels: None,
                encryption: None,
                cluster_size: None,
                raid_config: zero_strip,
            },
            None,
        )
        .await
        .expect_err("Zero strip size should fail");

    // Test 3: Multi-device without raid_config should fail
    pool_client
        .create(
            &v0::CreatePool {
                node: cluster.node(0),
                id: "multi-no-raid".into(),
                disks: vec![
                    "malloc:///disk3?size_mb=64".into(),
                    "malloc:///disk4?size_mb=64".into(),
                ],
                labels: None,
                encryption: None,
                cluster_size: None,
                raid_config: None, // Multi-device requires raid config
            },
            None,
        )
        .await
        .expect_err("Multi-device without raid config should fail");
}
