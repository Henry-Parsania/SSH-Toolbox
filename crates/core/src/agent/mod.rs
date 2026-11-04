//! The agent: a language model that can look around on a server and, with the user's approval,
//! change things. This module holds the parts that don't depend on the screen: talking to the
//! model (`llm`), the user's providers (`providers`), what may be run (`policy`) and removing
//! secrets from what the model sees (`redact`).

pub mod llm;
pub mod policy;
pub mod providers;
pub mod redact;
pub mod tools;

#[cfg(test)]
mod llm_tests;

#[cfg(test)]
mod exec_tests;
