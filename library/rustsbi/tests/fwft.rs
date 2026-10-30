use std::cell::Cell;

use rustsbi::spec::base::UNAVAILABLE_EXTENSION;
use rustsbi::spec::binary::{Error, SbiRet};
use rustsbi::spec::fwft::{FeatureType, Fid};
use rustsbi::{Configuration, Feature, Fwft, Lock};

fn config(feature: Feature, value: usize) -> Configuration {
    Configuration::from_parts(feature, value).expect("valid test configuration")
}

struct MockFwft {
    set_calls: Cell<usize>,
    get_calls: Cell<usize>,
    last_set: Cell<Option<(Configuration, Lock)>>,
    last_get: Cell<Option<Feature>>,
    set_result: Cell<Result<(), Error>>,
    get_result: Cell<Result<Configuration, Error>>,
}

impl Default for MockFwft {
    fn default() -> Self {
        Self {
            set_calls: Cell::new(0),
            get_calls: Cell::new(0),
            last_set: Cell::new(None),
            last_get: Cell::new(None),
            set_result: Cell::new(Ok(())),
            get_result: Cell::new(Ok(config(
                Feature::Standard(FeatureType::MisalignedExcDeleg),
                0,
            ))),
        }
    }
}

impl MockFwft {
    fn with_set_result(result: Result<(), Error>) -> Self {
        let mock = Self::default();
        mock.set_result.set(result);
        mock
    }

    fn with_get_result(result: Result<Configuration, Error>) -> Self {
        let mock = Self::default();
        mock.get_result.set(result);
        mock
    }
}

impl Fwft for MockFwft {
    fn set(&self, configuration: Configuration, lock: Lock) -> Result<(), Error> {
        self.set_calls.set(self.set_calls.get() + 1);
        self.last_set.set(Some((configuration, lock)));
        self.set_result.get()
    }

    fn get(&self, feature: Feature) -> Result<Configuration, Error> {
        self.get_calls.set(self.get_calls.get() + 1);
        self.last_get.set(Some(feature));
        self.get_result.get()
    }
}

fn args(feature: usize, value: usize, flags: usize) -> [usize; 6] {
    [feature, value, flags, 0, 0, 0]
}

fn call_set(fwft: &impl Fwft, feature: u32, value: usize, flags: usize) -> SbiRet {
    rustsbi::_rustsbi_fwft(
        fwft,
        args(feature as usize, value, flags),
        Fid::Set.as_usize(),
    )
}

fn call_get(fwft: &impl Fwft, feature: u32) -> SbiRet {
    rustsbi::_rustsbi_fwft(fwft, args(feature as usize, 0, 0), Fid::Get.as_usize())
}

#[test]
fn standard_features_decode_and_encode() {
    let cases = [
        (0, Feature::Standard(FeatureType::MisalignedExcDeleg)),
        (1, Feature::Standard(FeatureType::LandingPad)),
        (2, Feature::Standard(FeatureType::ShadowStack)),
        (3, Feature::Standard(FeatureType::DoubleTrap)),
        (4, Feature::Standard(FeatureType::PteAdHwUpdating)),
        (5, Feature::Standard(FeatureType::PointerMaskingPmlen)),
    ];
    for (id, feature) in cases {
        assert_eq!(Feature::from_id(id), Ok(feature));
        assert_eq!(feature.id(), id);
        assert_eq!(u32::from(feature), id);
        assert_eq!(Feature::try_from(id), Ok(feature));
    }
}

#[test]
fn boolean_configurations_validate_zero_and_one() {
    let cases = [
        Feature::Standard(FeatureType::MisalignedExcDeleg),
        Feature::Standard(FeatureType::LandingPad),
        Feature::Standard(FeatureType::ShadowStack),
        Feature::Standard(FeatureType::DoubleTrap),
        Feature::Standard(FeatureType::PteAdHwUpdating),
    ];
    for feature in cases {
        assert!(feature.is_boolean());
        let disabled = config(feature, 0);
        let enabled = config(feature, 1);
        assert_eq!(disabled.feature(), feature);
        assert_eq!(disabled.value(), 0);
        assert_eq!(enabled.value(), 1);
        assert_eq!(
            Configuration::from_parts(feature, 2),
            Err(Error::InvalidParam)
        );
    }
    assert!(!Feature::Standard(FeatureType::PointerMaskingPmlen).is_boolean());
}

#[test]
fn pointer_masking_preserves_requested_value() {
    for pmlen in [0, 1, 7, 16, usize::BITS as usize] {
        let configuration = config(Feature::Standard(FeatureType::PointerMaskingPmlen), pmlen);
        assert_eq!(
            configuration.feature(),
            Feature::Standard(FeatureType::PointerMaskingPmlen)
        );
        assert_eq!(configuration.value(), pmlen);
    }
}

#[test]
fn platform_features_preserve_identifier_scope_and_opaque_value() {
    for (id, is_global) in [
        (0x4000_0000, false),
        (0x7fff_ffff, false),
        (0xc000_0000, true),
        (0xffff_ffff, true),
    ] {
        let feature = Feature::from_id(id).unwrap();
        let Feature::PlatformSpecific(platform) = feature else {
            panic!("platform feature {id:#x} decoded as {feature:?}");
        };
        assert_eq!(platform, id);
        assert_eq!(feature.is_global(), is_global);
        assert_eq!(feature.id(), id);

        let configuration = config(feature, usize::MAX);
        assert_eq!(configuration.feature(), feature);
        assert_eq!(configuration.value(), usize::MAX);
    }
}

#[test]
fn reserved_identifiers_are_denied() {
    for id in [6, 0x3fff_ffff, 0x8000_0000, 0xbfff_ffff] {
        assert_eq!(Feature::from_id(id), Err(Error::Denied));
    }
}

#[test]
fn lock_decodes_only_zero_and_the_lock_bit() {
    assert_eq!(Lock::from_flags(0), Ok(Lock::Unchanged));
    assert_eq!(Lock::from_flags(1), Ok(Lock::Lock));
    assert_eq!(Lock::try_from(0), Ok(Lock::Unchanged));
    assert_eq!(usize::from(Lock::Unchanged), 0);
    assert_eq!(usize::from(Lock::Lock), 1);
    for flags in [2, 3, usize::MAX] {
        assert_eq!(Lock::from_flags(flags), Err(Error::InvalidParam));
    }
}

#[test]
fn malformed_parameters_never_reach_service() {
    let mock = MockFwft::default();

    assert_eq!(call_set(&mock, 0, 2, 0), SbiRet::invalid_param());
    for flags in [2, 3, usize::MAX] {
        assert_eq!(call_set(&mock, 0, 1, flags), SbiRet::invalid_param());
    }
    for feature in [6, 0x3fff_ffff, 0x8000_0000, 0xbfff_ffff] {
        assert_eq!(call_set(&mock, feature, 0, 0), SbiRet::denied());
        assert_eq!(call_get(&mock, feature), SbiRet::denied());
    }
    assert_eq!(
        rustsbi::_rustsbi_fwft(&mock, args(0, 1, 0), 2),
        SbiRet::not_supported()
    );
    assert_eq!(
        rustsbi::_rustsbi_fwft(&mock, args(0, 0, 0), 2),
        SbiRet::not_supported()
    );

    if let Some(too_large) = (u32::MAX as usize).checked_add(1) {
        assert_eq!(
            rustsbi::_rustsbi_fwft(&mock, args(too_large, 0, 0), Fid::Set.as_usize()),
            SbiRet::invalid_param()
        );
        assert_eq!(
            rustsbi::_rustsbi_fwft(&mock, args(too_large, 0, 0), Fid::Get.as_usize()),
            SbiRet::invalid_param()
        );
    }

    assert_eq!(mock.set_calls.get(), 0);
    assert_eq!(mock.get_calls.get(), 0);
}

#[test]
fn set_routes_typed_configuration_to_service() {
    let mock = MockFwft::default();

    assert_eq!(call_set(&mock, 3, 1, 1), SbiRet::success(0));
    assert_eq!(
        mock.last_set.get(),
        Some((
            config(Feature::Standard(FeatureType::DoubleTrap), 1),
            Lock::Lock
        ))
    );

    assert_eq!(call_set(&mock, 4, 0, 0), SbiRet::success(0));
    assert_eq!(
        mock.last_set.get(),
        Some((
            config(Feature::Standard(FeatureType::PteAdHwUpdating), 0),
            Lock::Unchanged
        ))
    );

    assert_eq!(call_set(&mock, 5, 7, 0), SbiRet::success(0));
    assert_eq!(
        mock.last_set.get(),
        Some((
            config(Feature::Standard(FeatureType::PointerMaskingPmlen), 7),
            Lock::Unchanged
        ))
    );

    let local = 0x4000_0000;
    assert_eq!(call_set(&mock, local, usize::MAX, 0), SbiRet::success(0));
    let (configuration, _) = mock
        .last_set
        .get()
        .expect("platform set was forwarded to the service");
    assert_eq!(configuration.feature(), Feature::PlatformSpecific(local));
    assert_eq!(configuration.value(), usize::MAX);

    assert_eq!(mock.set_calls.get(), 4);
}

#[test]
fn get_routes_feature_and_rejects_wrong_configuration() {
    let mock = MockFwft::default();

    mock.get_result
        .set(Ok(config(Feature::Standard(FeatureType::LandingPad), 1)));
    assert_eq!(call_get(&mock, 1), SbiRet::success(1));
    assert_eq!(
        mock.last_get.get(),
        Some(Feature::Standard(FeatureType::LandingPad))
    );

    mock.get_result.set(Ok(config(
        Feature::Standard(FeatureType::MisalignedExcDeleg),
        1,
    )));
    assert_eq!(call_get(&mock, 1), SbiRet::failed());

    let local = 0x4000_0000;
    let global = 0xc000_0000;
    mock.get_result
        .set(Ok(config(Feature::PlatformSpecific(global), 9)));
    assert_eq!(call_get(&mock, local), SbiRet::failed());

    mock.get_result
        .set(Ok(config(Feature::PlatformSpecific(local), 9)));
    assert_eq!(call_get(&mock, local), SbiRet::success(9));
}

#[test]
fn service_errors_map_to_sbi_ret() {
    let cases = [
        (Error::Failed, SbiRet::failed()),
        (Error::NotSupported, SbiRet::not_supported()),
        (Error::InvalidParam, SbiRet::invalid_param()),
        (Error::Denied, SbiRet::denied()),
        (Error::DeniedLocked, SbiRet::denied_locked()),
        (Error::InvalidState, SbiRet::invalid_state()),
    ];
    for (error, expected) in cases {
        let mock = MockFwft::with_set_result(Err(error));
        assert_eq!(call_set(&mock, 0, 1, 0), expected);

        let mock = MockFwft::with_get_result(Err(error));
        assert_eq!(call_get(&mock, 0), expected);
    }
}

#[test]
fn optional_and_reference_implementations_forward() {
    let none: Option<MockFwft> = None;
    assert_eq!(call_set(&none, 0, 1, 0), SbiRet::not_supported());
    assert_eq!(call_get(&none, 0), SbiRet::not_supported());
    assert_eq!(rustsbi::_rustsbi_fwft_probe(&none), UNAVAILABLE_EXTENSION);

    let some = Some(MockFwft::default());
    assert_eq!(call_set(&some, 0, 1, 0), SbiRet::success(0));
    some.as_ref().unwrap().get_result.set(Ok(config(
        Feature::Standard(FeatureType::MisalignedExcDeleg),
        1,
    )));
    assert_eq!(call_get(&some, 0), SbiRet::success(1));
    assert_eq!(
        rustsbi::_rustsbi_fwft_probe(&some),
        UNAVAILABLE_EXTENSION.wrapping_add(1)
    );

    let mock = MockFwft::default();
    assert_eq!(
        rustsbi::_rustsbi_fwft(&&mock, args(0, 1, 0), Fid::Set.as_usize()),
        SbiRet::success(0)
    );
    assert_eq!(mock.set_calls.get(), 1);
    assert_eq!(
        rustsbi::_rustsbi_fwft_probe(&&mock),
        UNAVAILABLE_EXTENSION.wrapping_add(1)
    );
}

struct QuestionMarkFwft {
    inner: MockFwft,
}

impl Fwft for QuestionMarkFwft {
    fn set(&self, configuration: Configuration, lock: Lock) -> Result<(), Error> {
        self.inner.set(configuration, lock)?;
        Ok(())
    }

    fn get(&self, feature: Feature) -> Result<Configuration, Error> {
        let configuration = self.inner.get(feature)?;
        Ok(configuration)
    }
}

#[test]
fn implementations_can_use_question_mark() {
    let fwft = QuestionMarkFwft {
        inner: MockFwft::default(),
    };
    assert_eq!(call_set(&fwft, 0, 1, 0), SbiRet::success(0));

    fwft.inner.get_result.set(Ok(config(
        Feature::Standard(FeatureType::MisalignedExcDeleg),
        1,
    )));
    assert_eq!(call_get(&fwft, 0), SbiRet::success(1));
}
