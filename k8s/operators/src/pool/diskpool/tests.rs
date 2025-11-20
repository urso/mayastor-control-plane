//! Unit tests for DiskPool CRD → REST API translation logic.

#[cfg(test)]
mod diskpool_tests {
    use super::super::crd::quantity::Quantity;
    use super::super::crd::v1beta3::{DiskPoolSpec, Raid0Config, RaidConfig};

    #[cfg(feature = "openapi")]
    use openapi::models::CreatePoolBody;

    /// Test single disk translation (no RAID config)
    #[cfg(feature = "openapi")]
    #[test]
    fn test_single_disk_translation() {
        let spec = DiskPoolSpec::new(
            "worker-1".to_string(),
            vec!["/dev/sda".to_string()],
            None,
            None,
            None,
            None,
            None,
        );

        // Simulate the translation logic from context.rs:280-282
        let raid_config = spec.raid_config.as_ref().map(|r| r.clone().into());
        let body = CreatePoolBody::new_all(spec.disks(), None, None, None, None, raid_config);

        // Assert expected hardcoded values
        assert_eq!(body.disks, vec!["/dev/sda"]);
        assert!(body.raid_config.is_none());
    }

    /// Test multiple disks with RAID0 translation
    #[cfg(feature = "openapi")]
    #[test]
    fn test_multiple_disks_raid0_translation() {
        let strip_size = Quantity::from_bytes(64 * 1024); // 64KB input
        let raid0_config = Raid0Config { strip_size };
        let spec = DiskPoolSpec::new(
            "worker-1".to_string(),
            vec!["/dev/sda".to_string(), "/dev/sdb".to_string()],
            None,
            None,
            None,
            None,
            Some(RaidConfig::Raid0(raid0_config)),
        );

        // Simulate the translation logic from context.rs:280-282
        let raid_config = spec.raid_config.as_ref().map(|r| r.clone().into());
        let body = CreatePoolBody::new_all(spec.disks(), None, None, None, None, raid_config);

        // Assert expected hardcoded values
        assert_eq!(body.disks, vec!["/dev/sda", "/dev/sdb"]);
        assert!(body.raid_config.is_some());

        // Verify the RAID config translation with hardcoded expectation
        match &body.raid_config {
            Some(openapi::models::RaidConfig::raid0(raid0)) => {
                assert_eq!(raid0.strip_size_kb, 64); // Hardcoded: 64KB expected
            }
            _ => panic!("Expected RAID0 config in CreatePoolBody"),
        }
    }

    /// Test RAID0 strip size conversion (128KB bytes → 128 KB)
    #[cfg(feature = "openapi")]
    #[test]
    fn test_raid0_strip_size_conversion() {
        let strip_size = Quantity::from_bytes(128 * 1024); // 128KB input
        let raid0_config = Raid0Config { strip_size };
        let spec = DiskPoolSpec::new(
            "worker-1".to_string(),
            vec!["/dev/nvme0n1".to_string(), "/dev/nvme1n1".to_string()],
            None,
            None,
            None,
            None,
            Some(RaidConfig::Raid0(raid0_config)),
        );

        // Simulate the translation logic from context.rs:280-282
        let raid_config = spec.raid_config.as_ref().map(|r| r.clone().into());
        let body = CreatePoolBody::new_all(spec.disks(), None, None, None, None, raid_config);

        // Verify bytes-to-KB conversion with hardcoded expectation
        match &body.raid_config {
            Some(openapi::models::RaidConfig::raid0(raid0)) => {
                assert_eq!(raid0.strip_size_kb, 128); // Hardcoded: 128 KB expected
            }
            _ => panic!("Expected RAID0 config with 128KB strip size"),
        }
    }
}
