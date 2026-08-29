use core::mem::{align_of, size_of};

use crate::{
    Version, base, clock, cppc, device_power, hsm, management_mode,
    message::{
        EnableNotificationRequest, EnableNotificationResponse, EventHeader, MessageHeader,
        MessageType, Status,
    },
    mpxy, performance, ras_agent, request_forward, shared_memory, system_msi, system_reset,
    system_suspend, voltage,
};

#[test]
fn message_layouts_are_word_sized() {
    assert_eq!(size_of::<MessageHeader>(), 8);
    assert_eq!(align_of::<MessageHeader>(), 4);
    assert_eq!(size_of::<EventHeader>(), 4);
    assert_eq!(align_of::<EventHeader>(), 4);
    assert_eq!(size_of::<base::Attributes>(), 4);
    assert_eq!(align_of::<base::Attributes>(), 4);
    assert_eq!(size_of::<voltage::Attributes>(), 4);
    assert_eq!(align_of::<voltage::Attributes>(), 4);
    assert_eq!(size_of::<voltage::ConfigFlags>(), 4);
    assert_eq!(align_of::<voltage::ConfigFlags>(), 4);
    assert_eq!(size_of::<clock::RateFormat>(), 4);
    assert_eq!(align_of::<clock::RateFormat>(), 4);
    assert_eq!(size_of::<clock::ConfigFlags>(), 4);
    assert_eq!(align_of::<clock::ConfigFlags>(), 4);
    assert_eq!(size_of::<clock::SetRateFlags>(), 4);
    assert_eq!(align_of::<clock::SetRateFlags>(), 4);
    assert_eq!(size_of::<clock::RoundingMode>(), 1);
    assert_eq!(size_of::<device_power::StateValue>(), 2);
    assert_eq!(size_of::<hsm::SuspendInfoFlags>(), 4);
    assert_eq!(align_of::<hsm::SuspendInfoFlags>(), 4);
    assert_eq!(size_of::<performance::EventId>(), 1);
    assert_eq!(size_of::<performance::Attributes>(), 4);
    assert_eq!(align_of::<performance::Attributes>(), 4);
    assert_eq!(size_of::<performance::DoorbellWidth>(), 1);
    assert_eq!(size_of::<performance::FastChannelAttributes>(), 4);
    assert_eq!(align_of::<performance::FastChannelAttributes>(), 4);
    assert_eq!(size_of::<ras_agent::DescriptorFormat>(), 1);
    assert_eq!(size_of::<ras_agent::DescriptorFlags>(), 4);
    assert_eq!(align_of::<ras_agent::DescriptorFlags>(), 4);
    assert_eq!(size_of::<system_msi::State>(), 4);
    assert_eq!(align_of::<system_msi::State>(), 4);
    assert_eq!(size_of::<system_msi::Attributes>(), 4);
    assert_eq!(align_of::<system_msi::Attributes>(), 4);
    assert_eq!(size_of::<EnableNotificationRequest>(), 8);
    assert_eq!(size_of::<EnableNotificationResponse>(), 8);
    assert_eq!(size_of::<Status>(), 4);
    assert_eq!(size_of::<MessageType>(), 1);
}

#[test]
fn message_header_packs_logical_words() {
    let header = MessageHeader::new(0x1234, 0x56, 0x08, 0x7898, 0xbcde).unwrap();

    assert_eq!(header.words(), [0x0856_1234, 0xbcde_7898]);
    assert_eq!(header.service_group_id(), 0x1234);
    assert_eq!(header.service_id(), 0x56);
    assert_eq!(header.flags(), 0x08);
    assert_eq!(header.message_type(), Ok(MessageType::NormalRequest));
    assert_eq!(header.data_len(), 0x7898);
    assert_eq!(header.token(), 0xbcde);
    assert_eq!(MessageHeader::from_words(header.words()), header);

    assert_eq!(MessageHeader::new(1, 1, 0, 2, 0), None);
    assert_eq!(MessageHeader::new(1, 1, 4, 0, 0), None);
    assert_eq!(MessageHeader::new(1, 1, 3, 0, 0), None);
    assert_eq!(MessageHeader::new(1, 0, 0, 0, 0), None);
    assert!(MessageHeader::new(1, 0, 3, 0, 0).is_some());
}

#[test]
fn protocol_enums_reject_unknown_values() {
    assert_eq!(MessageType::try_from(0), Ok(MessageType::NormalRequest));
    assert_eq!(MessageType::try_from(3), Ok(MessageType::Notification));
    assert_eq!(MessageType::try_from(7), Err(7));
    assert_eq!(MessageType::try_from(8), Err(8));

    let reserved_type = MessageHeader::from_fields(1, 1, 7, 0, 0);
    assert_eq!(reserved_type.message_type(), Err(7));

    assert_eq!(Status::try_from(0), Ok(Status::Success));
    assert_eq!(Status::try_from(u32::MAX), Ok(Status::Failed));
    assert_eq!(Status::try_from((-14_i32) as u32), Ok(Status::NoData));
    assert_eq!(Status::try_from((-15_i32) as u32), Err((-15_i32) as u32));
    assert_eq!(Status::try_from((-128_i32) as u32), Err((-128_i32) as u32));
    assert_eq!(Status::try_from(1), Err(1));
    assert_eq!(Status::NoData.code(), -14);
    assert_eq!(Status::NoData.bits(), (-14_i32) as u32);
    assert_eq!(u32::from(Status::Failed), u32::MAX);
    assert!(Status::Success.is_success());
    assert!(!Status::NoData.is_success());
}

#[test]
fn event_header_packs_logical_word() {
    let event = EventHeader::new(0x56, 0x1234).unwrap();
    assert_eq!(event.word(), 0x0056_1234);
    assert_eq!(event.event_id(), 0x56);
    assert_eq!(event.data_len(), 0x1234);

    let with_reserved = EventHeader::from_word(0xaa56_1234);
    assert_eq!(with_reserved.word(), 0xaa56_1234);
    assert_eq!(with_reserved.event_id(), 0x56);

    assert_eq!(EventHeader::new(1, 2), None);
}

#[test]
fn rpmi_version_uses_sixteen_bit_components() {
    let version = Version::new(1, 0);
    assert_eq!(version, base::SPEC_VERSION);
    assert_eq!(version.bits(), 0x0001_0000);
    assert_eq!(version.major(), 1);
    assert_eq!(version.minor(), 0);
    assert_eq!(Version::from_bits(version.bits()), version);
    assert_eq!(
        [
            base::SERVICE_GROUP_VERSION,
            system_msi::SERVICE_GROUP_VERSION,
            system_reset::SERVICE_GROUP_VERSION,
            system_suspend::SERVICE_GROUP_VERSION,
            hsm::SERVICE_GROUP_VERSION,
            cppc::SERVICE_GROUP_VERSION,
            voltage::SERVICE_GROUP_VERSION,
            clock::SERVICE_GROUP_VERSION,
            device_power::SERVICE_GROUP_VERSION,
            performance::SERVICE_GROUP_VERSION,
            management_mode::SERVICE_GROUP_VERSION,
            ras_agent::SERVICE_GROUP_VERSION,
            request_forward::SERVICE_GROUP_VERSION,
        ],
        [Version::new(1, 0); 13]
    );
}

#[test]
fn standard_service_group_ids_are_complete() {
    assert_eq!(
        [
            base::SERVICE_GROUP_ID,
            system_msi::SERVICE_GROUP_ID,
            system_reset::SERVICE_GROUP_ID,
            system_suspend::SERVICE_GROUP_ID,
            hsm::SERVICE_GROUP_ID,
            cppc::SERVICE_GROUP_ID,
            voltage::SERVICE_GROUP_ID,
            clock::SERVICE_GROUP_ID,
            device_power::SERVICE_GROUP_ID,
            performance::SERVICE_GROUP_ID,
            management_mode::SERVICE_GROUP_ID,
            ras_agent::SERVICE_GROUP_ID,
            request_forward::SERVICE_GROUP_ID,
        ],
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]
    );
}

#[test]
fn service_ids_match_ratified_v1() {
    assert_eq!(
        [
            base::ENABLE_NOTIFICATION,
            base::GET_IMPLEMENTATION_VERSION,
            base::GET_IMPLEMENTATION_ID,
            base::GET_SPEC_VERSION,
            base::GET_PLATFORM_INFO,
            base::PROBE_SERVICE_GROUP,
            base::GET_ATTRIBUTES,
        ],
        [1, 2, 3, 4, 5, 6, 7]
    );
    assert_eq!(
        [
            system_msi::ENABLE_NOTIFICATION,
            system_msi::GET_ATTRIBUTES,
            system_msi::GET_MSI_ATTRIBUTES,
            system_msi::SET_MSI_STATE,
            system_msi::GET_MSI_STATE,
            system_msi::SET_MSI_TARGET,
            system_msi::GET_MSI_TARGET,
        ],
        [1, 2, 3, 4, 5, 6, 7]
    );
    assert_eq!(
        [
            system_reset::ENABLE_NOTIFICATION,
            system_reset::GET_ATTRIBUTES,
            system_reset::RESET,
        ],
        [1, 2, 3]
    );
    assert_eq!(
        [
            system_suspend::ENABLE_NOTIFICATION,
            system_suspend::GET_ATTRIBUTES,
            system_suspend::SUSPEND,
        ],
        [1, 2, 3]
    );
    assert_eq!(
        [
            hsm::ENABLE_NOTIFICATION,
            hsm::GET_HART_STATUS,
            hsm::GET_HART_LIST,
            hsm::GET_SUSPEND_TYPES,
            hsm::GET_SUSPEND_INFO,
            hsm::HART_START,
            hsm::HART_STOP,
            hsm::HART_SUSPEND,
        ],
        [1, 2, 3, 4, 5, 6, 7, 8]
    );
    assert_eq!(
        [
            cppc::ENABLE_NOTIFICATION,
            cppc::PROBE_REG,
            cppc::READ_REG,
            cppc::WRITE_REG,
            cppc::GET_FAST_CHANNEL_REGION,
            cppc::GET_FAST_CHANNEL_OFFSET,
            cppc::GET_HART_LIST,
        ],
        [1, 2, 3, 4, 5, 6, 7]
    );
    assert_eq!(
        [
            voltage::ENABLE_NOTIFICATION,
            voltage::GET_NUM_DOMAINS,
            voltage::GET_ATTRIBUTES,
            voltage::GET_SUPPORTED_LEVELS,
            voltage::SET_CONFIG,
            voltage::GET_CONFIG,
            voltage::SET_LEVEL,
            voltage::GET_LEVEL,
        ],
        [1, 2, 3, 4, 5, 6, 7, 8]
    );
    assert_eq!(
        [
            clock::ENABLE_NOTIFICATION,
            clock::GET_NUM_CLOCKS,
            clock::GET_ATTRIBUTES,
            clock::GET_SUPPORTED_RATES,
            clock::SET_CONFIG,
            clock::GET_CONFIG,
            clock::SET_RATE,
            clock::GET_RATE,
        ],
        [1, 2, 3, 4, 5, 6, 7, 8]
    );
    assert_eq!(
        [
            device_power::ENABLE_NOTIFICATION,
            device_power::GET_NUM_DOMAINS,
            device_power::GET_ATTRIBUTES,
            device_power::SET_STATE,
            device_power::GET_STATE,
        ],
        [1, 2, 3, 4, 5]
    );
    assert_eq!(
        [
            performance::ENABLE_NOTIFICATION,
            performance::GET_NUM_DOMAINS,
            performance::GET_ATTRIBUTES,
            performance::GET_SUPPORTED_LEVELS,
            performance::GET_LEVEL,
            performance::SET_LEVEL,
            performance::GET_LIMIT,
            performance::SET_LIMIT,
            performance::GET_FAST_CHANNEL_REGION,
            performance::GET_FAST_CHANNEL_ATTRIBUTES,
        ],
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    );
    assert_eq!(
        [
            management_mode::ENABLE_NOTIFICATION,
            management_mode::GET_ATTRIBUTES,
            management_mode::COMMUNICATE,
        ],
        [1, 2, 3]
    );
    assert_eq!(
        [
            ras_agent::ENABLE_NOTIFICATION,
            ras_agent::GET_NUM_ERROR_SOURCES,
            ras_agent::GET_ERROR_SOURCE_ID_LIST,
            ras_agent::GET_ERROR_SOURCE_DESCRIPTOR,
        ],
        [1, 2, 3, 4]
    );
    assert_eq!(
        [
            request_forward::ENABLE_NOTIFICATION,
            request_forward::RETRIEVE_CURRENT_MESSAGE,
            request_forward::COMPLETE_CURRENT_MESSAGE,
        ],
        [1, 2, 3]
    );
}

#[test]
fn subtle_v1_encodings_are_locked_down() {
    assert_eq!(hsm::hart_state::STARTED, sbi_spec::hsm::hart_state::STARTED);
    assert_eq!(
        hsm::hart_state::RESUME_PENDING,
        sbi_spec::hsm::hart_state::RESUME_PENDING
    );
    assert_eq!(
        hsm::suspend_type::RETENTIVE,
        sbi_spec::hsm::suspend_type::RETENTIVE
    );
    assert_eq!(
        hsm::suspend_type::NON_RETENTIVE,
        sbi_spec::hsm::suspend_type::NON_RETENTIVE
    );
    assert_eq!(shared_memory::DOORBELL_REQUEST, 0x08);
    assert_eq!(system_reset::attribute::SUPPORTED, 1 << 0);

    let base_attributes = base::Attributes::new(true, true);
    assert_eq!(base_attributes.bits(), 0b11);
    assert!(base_attributes.event_notification_supported());
    assert!(base_attributes.context_m_mode());
    assert_eq!(base_attributes.reserved_bits(), 0);

    let unknown_base_attributes = base::Attributes::from_bits(0b111);
    assert!(unknown_base_attributes.event_notification_supported());
    assert!(unknown_base_attributes.context_m_mode());
    assert_eq!(unknown_base_attributes.reserved_bits(), 0b100);

    let system_msi_state = system_msi::State::new(true);
    assert_eq!(system_msi_state.bits(), 1);
    assert!(system_msi_state.enabled());
    assert!(!system_msi_state.pending());
    assert_eq!(system_msi_state.reserved_bits(), 0);

    let reported_system_msi_state = system_msi::State::from_bits(0b111);
    assert!(reported_system_msi_state.enabled());
    assert!(reported_system_msi_state.pending());
    assert_eq!(reported_system_msi_state.reserved_bits(), 0b100);

    let system_msi_attributes = system_msi::Attributes::new(true);
    assert_eq!(system_msi_attributes.bits(), 1);
    assert!(system_msi_attributes.preferred_m_mode());
    assert_eq!(system_msi_attributes.reserved_bits(), 0);

    let unknown_system_msi_attributes = system_msi::Attributes::from_bits(3);
    assert!(unknown_system_msi_attributes.preferred_m_mode());
    assert_eq!(unknown_system_msi_attributes.reserved_bits(), 2);

    let voltage_attributes = voltage::Attributes::new(voltage::LevelFormat::LinearRange, false);
    assert_eq!(voltage_attributes.bits(), 0x02);
    assert!(!voltage_attributes.always_on());
    assert_eq!(
        voltage_attributes.level_format(),
        Ok(voltage::LevelFormat::LinearRange)
    );
    assert_eq!(voltage_attributes.reserved_bits(), 0);
    assert_eq!(voltage::LevelFormat::try_from(2), Err(2));

    let unknown_voltage_attributes = voltage::Attributes::from_bits(0x14);
    assert_eq!(unknown_voltage_attributes.level_format(), Err(2));
    assert_eq!(unknown_voltage_attributes.reserved_bits(), 0x10);

    let voltage_config = voltage::ConfigFlags::new(true);
    assert_eq!(voltage_config.bits(), 1);
    assert!(voltage_config.enabled());
    assert_eq!(voltage_config.reserved_bits(), 0);

    let unknown_voltage_config = voltage::ConfigFlags::from_bits(3);
    assert!(unknown_voltage_config.enabled());
    assert_eq!(unknown_voltage_config.reserved_bits(), 2);

    assert_eq!(clock::RateFormat::LinearRange.bits(), 1);
    assert_eq!(
        clock::RateFormat::try_from(0),
        Ok(clock::RateFormat::Discrete)
    );
    assert_eq!(
        clock::RateFormat::try_from(1),
        Ok(clock::RateFormat::LinearRange)
    );
    assert_eq!(clock::RateFormat::try_from(2), Err(2));
    assert_eq!(clock::RateFormat::try_from(1 << 2), Err(1 << 2));

    let clock_config = clock::ConfigFlags::new(true);
    assert_eq!(clock_config.bits(), 1);
    assert!(clock_config.enabled());
    assert_eq!(clock_config.reserved_bits(), 0);

    let unknown_clock_config = clock::ConfigFlags::from_bits(3);
    assert!(unknown_clock_config.enabled());
    assert_eq!(unknown_clock_config.reserved_bits(), 2);

    let set_rate_flags = clock::SetRateFlags::new(clock::RoundingMode::Auto);
    assert_eq!(set_rate_flags.bits(), 0b10);
    assert_eq!(
        set_rate_flags.rounding_mode(),
        Ok(clock::RoundingMode::Auto)
    );
    assert_eq!(set_rate_flags.reserved_bits(), 0);
    assert_eq!(clock::RoundingMode::try_from(3), Err(3));

    let unknown_set_rate_flags = clock::SetRateFlags::from_bits(0b111);
    assert_eq!(unknown_set_rate_flags.rounding_mode(), Err(3));
    assert_eq!(unknown_set_rate_flags.reserved_bits(), 0b100);
    assert_eq!(device_power::StateValue::Off.bits(), 3);
    assert_eq!(
        device_power::StateValue::try_from(0),
        Ok(device_power::StateValue::On)
    );
    assert_eq!(
        device_power::StateValue::try_from(3),
        Ok(device_power::StateValue::Off)
    );
    assert_eq!(device_power::StateValue::try_from(1), Err(1));
    assert_eq!(device_power::CONTEXT_LOST, 1 << 16);
    assert_eq!(device_power::VENDOR_SPECIFIC_START, 0x1000);
    assert_eq!(device_power::VENDOR_SPECIFIC_END, 0xffff);
    assert_eq!(
        device_power::StateValue::try_from(device_power::VENDOR_SPECIFIC_START),
        Err(device_power::VENDOR_SPECIFIC_START)
    );
    assert_eq!(
        device_power::StateValue::try_from(device_power::VENDOR_SPECIFIC_END),
        Err(device_power::VENDOR_SPECIFIC_END)
    );
    let suspend_info = hsm::SuspendInfoFlags::new(true);
    assert_eq!(suspend_info.bits(), 1);
    assert!(suspend_info.local_timer_stops());
    assert_eq!(suspend_info.reserved_bits(), 0);

    let unknown_suspend_info = hsm::SuspendInfoFlags::from_bits(3);
    assert!(unknown_suspend_info.local_timer_stops());
    assert_eq!(unknown_suspend_info.reserved_bits(), 2);

    assert_eq!(performance::EventId::PowerChange.bits(), 1);
    assert_eq!(
        performance::EventId::try_from(3),
        Ok(performance::EventId::LevelChange)
    );
    assert_eq!(performance::EventId::try_from(0), Err(0));

    let performance_attributes = performance::Attributes::new(true, false, true);
    assert_eq!(performance_attributes.bits(), 0b101);
    assert!(performance_attributes.fast_channel_supported());
    assert!(!performance_attributes.level_change_supported());
    assert!(performance_attributes.limit_change_supported());
    assert_eq!(performance_attributes.reserved_bits(), 0);

    let unknown_performance_attributes = performance::Attributes::from_bits(0b1101);
    assert_eq!(unknown_performance_attributes.reserved_bits(), 0b1000);
    assert_eq!(cppc::fast_channel::DOORBELL_WIDTH_32, 0b10 << 1);

    let fast_channel_attributes =
        performance::FastChannelAttributes::new(true, performance::DoorbellWidth::Bits32);
    assert_eq!(fast_channel_attributes.bits(), 0b101);
    assert!(fast_channel_attributes.doorbell_supported());
    assert_eq!(
        fast_channel_attributes.doorbell_width(),
        Ok(performance::DoorbellWidth::Bits32)
    );
    assert_eq!(fast_channel_attributes.reserved_bits(), 0);
    assert_eq!(performance::DoorbellWidth::try_from(3), Err(3));

    let unknown_fast_channel_attributes = performance::FastChannelAttributes::from_bits(0b1111);
    assert_eq!(unknown_fast_channel_attributes.doorbell_width(), Err(3));
    assert_eq!(unknown_fast_channel_attributes.reserved_bits(), 0b1000);

    let descriptor_flags =
        ras_agent::DescriptorFlags::new(ras_agent::DescriptorFormat::ImplementationSpecific);
    assert_eq!(descriptor_flags.bits(), 0xf);
    assert_eq!(
        descriptor_flags.format(),
        Ok(ras_agent::DescriptorFormat::ImplementationSpecific)
    );
    assert_eq!(descriptor_flags.reserved_bits(), 0);
    assert_eq!(ras_agent::DescriptorFormat::try_from(1), Err(1));

    let unknown_descriptor_flags = ras_agent::DescriptorFlags::from_bits(0x21);
    assert_eq!(unknown_descriptor_flags.format(), Err(1));
    assert_eq!(unknown_descriptor_flags.reserved_bits(), 0x20);

    assert_eq!(performance::FastChannelAttributes::REGION_ALIGNMENT, 8);
    assert_eq!(performance::FastChannelAttributes::LEVEL_SIZE, 4);
    assert_eq!(performance::FastChannelAttributes::LIMIT_SIZE, 8);
    assert_eq!(performance::FastChannelAttributes::MAX_LEVEL_OFFSET, 0);
    assert_eq!(performance::FastChannelAttributes::MIN_LEVEL_OFFSET, 4);
}

#[test]
fn independent_record_layouts_match_word_counts() {
    assert_eq!(size_of::<clock::Rate>(), 8);
    assert_eq!(size_of::<clock::LinearRange>(), 24);
    assert_eq!(size_of::<voltage::LinearRange>(), 12);
    assert_eq!(size_of::<performance::Level>(), 16);

    let rate = clock::Rate::from_hz(0x0123_4567_89ab_cdef);
    assert_eq!(rate.words(), [0x89ab_cdef, 0x0123_4567]);
    assert_eq!(rate.hz(), 0x0123_4567_89ab_cdef);

    let state = device_power::PowerState::new(device_power::StateValue::Off, true);
    assert_eq!(state.bits(), 0x0001_0003);
    assert_eq!(state.value(), Ok(device_power::StateValue::Off));
    assert!(state.context_lost());

    let vendor_state = device_power::PowerState::from_bits(0x0000_1000);
    assert_eq!(
        vendor_state.value(),
        Err(device_power::VENDOR_SPECIFIC_START)
    );
}

#[test]
fn shared_memory_and_mpxy_constants_match_v1() {
    assert_eq!(shared_memory::MIN_SLOT_SIZE, 64);
    assert_eq!(shared_memory::HEAD_SLOT, 0);
    assert_eq!(shared_memory::TAIL_SLOT, 1);
    assert_eq!(shared_memory::MESSAGE_SLOT_START, 2);
    assert_eq!(shared_memory::INDEX_OFFSET, 0);
    assert_eq!(shared_memory::INDEX_SIZE, 4);

    assert_eq!(mpxy::attribute::SERVICE_GROUP_ID, 0x8000_0000);
    assert_eq!(mpxy::attribute::SERVICE_GROUP_VERSION, 0x8000_0001);
    assert_eq!(mpxy::attribute::IMPLEMENTATION_ID, 0x8000_0002);
    assert_eq!(mpxy::attribute::IMPLEMENTATION_VERSION, 0x8000_0003);
}
