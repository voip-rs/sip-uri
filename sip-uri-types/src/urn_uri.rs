//! The URN value.
//!
//! An empty r-, q- or f-component is held as `Some("")`, unlike an empty
//! SIP or tel: fragment, which is absent: `?+`, `?=` or `#` alone re-parses
//! as that empty component, and the rationale's absence rule holds a
//! component absent only where its delimiter alone re-parses as nothing.

use std::fmt;

use crate::canon;

/// URN (Uniform Resource Name) per RFC 8141.
///
/// Parse text into one with sip-uri's `UriParse`.
///
/// Represents `urn:NID:NSS` with optional resolution (`?+`), query (`?=`),
/// and fragment (`#`) components.
///
/// The NID is stored lowercase per RFC 8141 equivalence rules. No component
/// decodes an escape; escape hex is uppercase, and a byte outside the
/// component's grammar is escaped. A missing NID or NSS is `None`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(from = "UrnUriParts", into = "UrnUriParts")
)]
#[non_exhaustive]
pub struct UrnUri {
    nid: Option<String>,
    nss: Option<String>,
    r_component: Option<String>,
    q_component: Option<String>,
    f_component: Option<String>,
}

/// The components of a [`UrnUri`], each canonized on conversion.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct UrnUriParts {
    /// The namespace identifier.
    pub nid: Option<String>,
    /// The namespace-specific string.
    pub nss: Option<String>,
    /// The resolution component, after `?+`.
    pub r_component: Option<String>,
    /// The query component, after `?=`.
    pub q_component: Option<String>,
    /// The fragment component, after `#`.
    pub f_component: Option<String>,
}

impl From<UrnUriParts> for UrnUri {
    fn from(p: UrnUriParts) -> Self {
        UrnUri {
            nid: p
                .nid
                .as_deref()
                .and_then(hold_nid),
            nss: p
                .nss
                .as_deref()
                .and_then(hold_nss),
            r_component: p
                .r_component
                .as_deref()
                .map(canon::canonize_urn_r),
            q_component: p
                .q_component
                .as_deref()
                .map(canon::canonize_urn_q),
            f_component: p
                .f_component
                .as_deref()
                .map(canon::canonize_fragment),
        }
    }
}

fn hold_nid(nid: &str) -> Option<String> {
    (!nid.is_empty()).then(|| canon::canonize_nid(nid))
}

fn hold_nss(nss: &str) -> Option<String> {
    (!nss.is_empty()).then(|| canon::canonize_nss(nss))
}

impl From<UrnUri> for UrnUriParts {
    fn from(uri: UrnUri) -> Self {
        uri.into_parts()
    }
}

impl UrnUri {
    /// Create a new URN with the given NID and NSS.
    ///
    /// Both are canonized like parsed text, and the NID is lowercased.
    pub fn new(nid: impl AsRef<str>, nss: impl AsRef<str>) -> Self {
        UrnUri {
            nid: hold_nid(nid.as_ref()),
            nss: hold_nss(nss.as_ref()),
            r_component: None,
            q_component: None,
            f_component: None,
        }
    }

    /// The components, in canonical form.
    pub fn into_parts(self) -> UrnUriParts {
        UrnUriParts {
            nid: self.nid,
            nss: self.nss,
            r_component: self.r_component,
            q_component: self.q_component,
            f_component: self.f_component,
        }
    }

    /// Set the resolution component (`?+`).
    pub fn with_r_component(mut self, r: impl AsRef<str>) -> Self {
        self.r_component = Some(canon::canonize_urn_r(r.as_ref()));
        self
    }

    /// Set the query component (`?=`).
    pub fn with_q_component(mut self, q: impl AsRef<str>) -> Self {
        self.q_component = Some(canon::canonize_urn_q(q.as_ref()));
        self
    }

    /// Set the fragment component (`#`).
    pub fn with_f_component(mut self, f: impl AsRef<str>) -> Self {
        self.f_component = Some(canon::canonize_fragment(f.as_ref()));
        self
    }

    /// The Namespace Identifier (always lowercase).
    pub fn nid(&self) -> Option<&str> {
        self.nid
            .as_deref()
    }

    /// The Namespace Specific String.
    pub fn nss(&self) -> Option<&str> {
        self.nss
            .as_deref()
    }

    /// The resolution component, if present.
    pub fn r_component(&self) -> Option<&str> {
        self.r_component
            .as_deref()
    }

    /// The query component, if present.
    pub fn q_component(&self) -> Option<&str> {
        self.q_component
            .as_deref()
    }

    /// The fragment component, if present.
    pub fn f_component(&self) -> Option<&str> {
        self.f_component
            .as_deref()
    }

    /// Render the assigned name, `urn:NID:NSS`, as [`fmt::Display`] writes
    /// it, without the r-, q- and f-components.
    ///
    /// ```
    /// use sip_uri_types::UrnUri;
    ///
    /// let urn = UrnUri::new("service", "counseling").with_q_component("x");
    /// assert_eq!(urn.assigned_name().to_string(), "urn:service:counseling");
    /// ```
    pub fn assigned_name(&self) -> AssignedName<'_> {
        AssignedName(self)
    }
}

/// [`fmt::Display`] adapter returned by [`UrnUri::assigned_name`].
#[derive(Debug, Clone, Copy)]
pub struct AssignedName<'a>(&'a UrnUri);

impl fmt::Display for AssignedName<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let urn = self.0;
        f.write_str("urn:")?;
        if let Some(ref nid) = urn.nid {
            f.write_str(nid)?;
        }
        if let Some(ref nss) = urn.nss {
            write!(f, ":{nss}")?;
        }
        Ok(())
    }
}

impl fmt::Display for UrnUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.assigned_name())?;
        if let Some(ref r) = self.r_component {
            write!(f, "?+{r}")?;
        }
        if let Some(ref q) = self.q_component {
            write!(f, "?={q}")?;
        }
        if let Some(ref frag) = self.f_component {
            write!(f, "#{frag}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder() {
        let urn = UrnUri::new("service", "sos.fire");
        assert_eq!(urn.to_string(), "urn:service:sos.fire");
    }

    #[test]
    fn builder_with_components() {
        let urn = UrnUri::new("example", "resource")
            .with_r_component("resolve")
            .with_q_component("query")
            .with_f_component("section");
        assert_eq!(
            urn.to_string(),
            "urn:example:resource?+resolve?=query#section"
        );
    }

    #[test]
    fn parts_canonize_like_builders() {
        let urn = UrnUri::from(UrnUriParts {
            nid: Some("EXAMPLE".into()),
            nss: Some("a b".into()),
            r_component: Some("x?=y".into()),
            ..Default::default()
        });
        assert_eq!(urn, UrnUri::new("EXAMPLE", "a b").with_r_component("x?=y"));
        assert_eq!(urn.nid(), Some("example"));
        assert_eq!(
            UrnUri::from(
                urn.clone()
                    .into_parts()
            ),
            urn
        );
    }

    #[test]
    fn empty_nid_and_nss_are_absent() {
        assert_eq!(UrnUri::new("", "x").nid(), None);
        assert_eq!(UrnUri::new("x", "").nss(), None);
        let empty_rqf = UrnUri::new("example", "a")
            .with_r_component("")
            .with_q_component("")
            .with_f_component("");
        assert_eq!(empty_rqf.to_string(), "urn:example:a?+?=#");
    }
}
