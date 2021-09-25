//! Ring protocol logic and supporting types.

use std::{collections::BTreeMap, convert::TryFrom, fmt::Display, hash::Hasher};

use parking_lot::RwLock;

use crate::conn_manager::{self, PeerKeyLocation};

#[derive(Debug)]
pub(crate) struct Ring {
    pub connections_by_location: RwLock<BTreeMap<Location, PeerKeyLocation>>,
    pub rnd_if_htl_above: usize,
    pub max_hops_to_live: usize,
}

impl Ring {
    const MIN_CONNECTIONS: usize = 10;
    const MAX_CONNECTIONS: usize = 20;

    /// Above this number of remaining hops,
    /// randomize which of node a message which be forwarded to.
    pub const RAND_WALK_ABOVE_HTL: usize = 7;

    ///
    pub const MAX_HOPS_TO_LIVE: usize = 10;

    pub fn new() -> Self {
        Ring {
            connections_by_location: RwLock::new(BTreeMap::new()),
            rnd_if_htl_above: Self::RAND_WALK_ABOVE_HTL,
            max_hops_to_live: Self::MAX_HOPS_TO_LIVE,
        }
    }

    pub fn with_rnd_walk_above(&mut self, rnd_if_htl_above: usize) -> &mut Self {
        self.rnd_if_htl_above = rnd_if_htl_above;
        self
    }

    pub fn with_max_hops(&mut self, max_hops_to_live: usize) -> &mut Self {
        self.max_hops_to_live = max_hops_to_live;
        self
    }

    pub fn should_accept(&self, my_location: &Location, location: &Location) -> bool {
        let cbl = &*self.connections_by_location.read();
        if location == my_location || cbl.contains_key(location) {
            false
        } else if cbl.len() < Self::MIN_CONNECTIONS {
            true
        } else if cbl.len() >= Self::MAX_CONNECTIONS {
            false
        } else {
            my_location.distance(location) < self.median_distance_to(my_location)
        }
    }

    pub fn median_distance_to(&self, location: &Location) -> Distance {
        let mut conn_by_dist = self.connections_by_distance(location);
        conn_by_dist.sort_by_key(|(k, _)| *k);
        let idx = self.connections_by_location.read().len() / 2;
        conn_by_dist[idx].0
    }

    pub fn connections_by_distance(&self, to: &Location) -> Vec<(Distance, PeerKeyLocation)> {
        self.connections_by_location
            .read()
            .iter()
            .map(|(key, peer)| (key.distance(to), *peer))
            .collect()
    }

    pub fn random_peer<F>(&self, filter_fn: F) -> Option<PeerKeyLocation>
    where
        F: FnMut(&&PeerKeyLocation) -> bool,
    {
        // FIXME: should be optimized and avoid copying
        self.connections_by_location
            .read()
            .values()
            .find(filter_fn)
            .copied()
    }
}

/// An abstract location on the 1D ring, represented by a real number on the interal [0, 1]
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Copy)]
pub struct Location(f64);

pub(crate) type Distance = Location;

impl Location {
    /// Returns a new random location.
    pub fn random() -> Self {
        use rand::prelude::*;
        let mut rng = rand::thread_rng();
        Location(rng.gen_range(0.0..=1.0))
    }

    /// Compute the distance between two locations.
    pub fn distance(&self, other: &Location) -> Distance {
        let d = (self.0 - other.0).abs();
        if d < 0.5 {
            Location(d)
        } else {
            Location(1.0 - d)
        }
    }
}

impl Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.to_string().as_str())?;
        Ok(())
    }
}

impl PartialEq for Location {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

/// Since we don't allow NaN values in the construction of Location
/// we can safely assume that an equivalence relation holds.  
impl Eq for Location {}

impl Ord for Location {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other)
            .expect("always should return a cmp value")
    }
}

impl PartialOrd for Location {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl std::hash::Hash for Location {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let bits = self.0.to_bits();
        state.write_u64(bits);
        state.finish();
    }
}

impl TryFrom<f64> for Location {
    type Error = ();

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if !(0.0..=1.0).contains(&value) {
            Err(())
        } else {
            Ok(Location(value))
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum RingProtoError {
    #[error("failed while attempting to join a ring")]
    Join,
    #[error(transparent)]
    ConnError(#[from] Box<conn_manager::ConnError>),
}