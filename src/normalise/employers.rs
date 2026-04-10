use super::NormalisationResult;

pub fn apply(_result: &mut NormalisationResult) {
    // TODO: implement employer matching with salary-context regex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placeholder() {
        let mut r = NormalisationResult::new("TEST");
        apply(&mut r);
        assert!(r.features.entity_name.is_none());
    }
}
