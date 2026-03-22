# WCL - Wil's Configuration Language

set unstable

mod build '.just/build.just'
mod test '.just/test.just'
mod pack '.just/pack.just'
mod dev '.just/dev.just'
mod ci '.just/ci.just'
mod docs '.just/docs.just'
mod examples '.just/examples.just'

[private]
default:
    just --list --list-submodules
