//! Lazy selection of parsed device descriptions.

/// Selects a parsed device description from device-tree compatible strings.
///
/// The `first` form visits compatible strings in order and runs the first
/// matching arm once.
/// The `priority` form evaluates candidates in order and returns the first
/// available description.
/// The `for` form tries each candidate within a matching compatible string,
/// returning `$invalid` if a compatible matched but no candidate produced a
/// description.
macro_rules! match_desc {
    (first $compatible:ident in $compatibles:expr; {
        $($pattern:pat $(if $guard:expr)? => $body:block),+ $(,)?
    }) => {{
        for $compatible in $compatibles {
            match $compatible {
                $(
                    $pattern $(if $guard)? => {
                        $body
                        break;
                    }
                ),+
                _ => continue,
            }
        }
        Ok(())
    }};
    (priority $output:ty; $($candidate:expr),+ $(,)?) => {{
        'selected: {
            $(
                match ($candidate)? {
                    Some(description) => {
                        break 'selected Ok(Some(::alloc::boxed::Box::new(description) as $output));
                    }
                    None => {}
                }
            )+
            Ok(None)
        }
    }};
    (for $compatible:ident in $compatibles:expr, $invalid:expr; {
        $($pattern:pat => [$($candidate:expr),+ $(,)?]),+ $(,)?
    }) => {{
        let mut supported = false;
        let mut selected = None;
        for $compatible in $compatibles {
            let result = match $compatible {
                $(
                    $pattern => {
                        supported = true;
                        'candidate: {
                            $(
                                match ($candidate)? {
                                    Some(description) => break 'candidate Some(description),
                                    None => {}
                                }
                            )+
                            None
                        }
                    }
                ),+
                _ => continue,
            };
            if let Some(description) = result {
                selected = Some(description);
                break;
            }
        }
        match (selected, supported) {
            (Some(description), _) => Ok(Some(description)),
            (None, true) => Err($invalid),
            (None, false) => Ok(None),
        }
    }};
}

pub(crate) use match_desc;
