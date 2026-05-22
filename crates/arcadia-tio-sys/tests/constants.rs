use std::ffi::c_int;
use std::mem::{align_of, size_of};

use arcadia_tio_sys::*;

const _: () = {
    assert!(size_of::<ArcadiaTioDType>() == size_of::<c_int>());
    assert!(size_of::<ArcadiaTioErrorCode>() == size_of::<c_int>());
    assert!(ARCADIA_TIO_DTYPE_F32 == 0);
    assert!(ARCADIA_TIO_DTYPE_F64 == 1);
    assert!(ARCADIA_TIO_DTYPE_I32 == 2);
    assert!(ARCADIA_TIO_DTYPE_I64 == 3);
    assert!(ARCADIA_TIO_ERROR_OK == 0);
    assert!(ARCADIA_TIO_AXIS_TIME == 0);
    assert!(ARCADIA_TIO_AXIS_SYMBOL == 1);
    assert!(ARCADIA_TIO_COORDINATE_DTYPE_I32 == 0);
    assert!(ARCADIA_TIO_COORDINATE_KIND_DATE == 2);
    assert!(ARCADIA_TIO_COORDINATE_ENCODING_DATE_YYYYMMDD == 2);
    assert!(ARCADIA_TIO_COORDINATE_STORAGE_INLINE == 0);
    assert!(ARCADIA_TIO_HEADER_PROFILE_STREAMING == 0);
};

#[test]
fn representative_raw_layouts_are_pointer_compatible() {
    assert_eq!(align_of::<ArcadiaTioTensor>(), align_of::<usize>());
    assert_eq!(
        align_of::<ArcadiaTioAxisCoordinateInput>(),
        align_of::<usize>()
    );

    #[cfg(target_pointer_width = "64")]
    assert_eq!(size_of::<ArcadiaTioAxisCoordinateInput>(), 120);

    #[cfg(target_pointer_width = "32")]
    assert!(size_of::<ArcadiaTioAxisCoordinateInput>() >= 72);
}
