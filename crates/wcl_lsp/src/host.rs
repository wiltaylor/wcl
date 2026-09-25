//! The vocabulary the server opens every document with, supplied by
//! whoever starts it.

use wcl_lang::{Environment, FileLoader, Registry};

/// What the embedding program layers over plain WCL: the
/// [`Environment`] (host builtins, synthetic declarations and the
/// `@contextual` expander) and the [`Registry`] of system imports
/// (`import <name.wcl>`) every document opens with.
///
/// The server has no vocabulary of its own. [`Host::default`] serves
/// plain WCL; a host with a document DSL built on WCL passes its own
/// environment and registry, so its documents are checked the way its
/// build checks them.
#[derive(Clone)]
pub struct Host {
    /// The environment every document opens with.
    environment: Environment,
    /// System imports, consulted before the disk and the open buffers.
    registry: Registry,
}

impl Host {
    /// A host from its environment and system-import registry.
    pub fn new(environment: Environment, registry: Registry) -> Self {
        Self {
            environment,
            registry,
        }
    }

    /// The environment every document opens with.
    pub fn environment(&self) -> &Environment {
        &self.environment
    }

    /// A loader that serves this host's system imports and reads
    /// everything else through `fallback`.
    pub(crate) fn loader(&self, fallback: FileLoader) -> FileLoader {
        self.registry.clone().loader(fallback)
    }
}

impl Default for Host {
    /// Plain WCL: the language's own environment and no system imports.
    fn default() -> Self {
        Self::new(Environment::new(), Registry::new())
    }
}

/// The wdoc host, for tests that exercise wdoc documents.
#[cfg(test)]
pub(crate) fn wdoc() -> std::sync::Arc<Host> {
    std::sync::Arc::new(Host::new(
        wcl_wdoc::wdoc_environment(),
        wcl_wdoc::schema_registry(),
    ))
}
