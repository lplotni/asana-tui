//! The board's easter egg (spec F31): `g d` makes a random animal dance for
//! about 3 s. Pure state (ADR-002): frames come from the event loop every
//! 40 ms, as for F23's animation, and the event loop seeds the choice of
//! animal once at start.

/// Frames the dance lasts: 3 s of 40 ms frames.
pub const DANCE_FRAMES: u16 = 75;
/// Frames each pose is held (200 ms).
pub const POSE_FRAMES: u16 = 5;

/// An animal and its dance: poses shown in turn, each a few lines of ASCII
/// art (so a character is a cell).
pub struct Animal {
    pub name: &'static str,
    pub poses: &'static [&'static [&'static str]],
}

impl Animal {
    /// The width and height that hold every pose, so the box keeps its
    /// size while the animal dances.
    pub fn size(&self) -> (usize, usize) {
        let lines = || self.poses.iter().flat_map(|pose| pose.iter());
        let width = lines().map(|line| line.len()).max().unwrap_or(0);
        let height = self.poses.iter().map(|pose| pose.len()).max().unwrap_or(0);
        (width, height)
    }
}

const CAT_LEFT: &[&str] = &[r"  /\_/\", r"\( ^.^ )", r"  |   |\", r"  /   \"];
const CAT_UP: &[&str] = &[r"  /\_/\", r"\( ^o^ )/", r"  |   |", r" /     \"];
const CAT_RIGHT: &[&str] = &[r"  /\_/\", r" ( ^.^ )/", r" /|   |", r"  /   \"];

const FROG_SIT: &[&str] = &[r"  @..@", r" (----)", r"( >__< )", r"^^ ~~ ^^"];
const FROG_HOP: &[&str] = &[r"   @..@", r"  (----)", r" ( >__< )", r" ~~ ^^ ~~"];

const PENGUIN_RIGHT: &[&str] = &[r" (o>", r" //\", r" V_/_"];
const PENGUIN_LEFT: &[&str] = &[r" <o)", r" /\\", r"_\_V"];

const CHICK_RIGHT: &[&str] = &[r"   \\", r"   (o>", r"\\_//)", r" \_/_)", r"  _|_"];
const CHICK_LEFT: &[&str] = &[r" //", r"<o)", r"(\\_//", r"(_\_/", r" _|_"];

const CRAB_CLOSED: &[&str] = &[r" (\/)(o,,,o)(\/)", r"     //   \\"];
const CRAB_OPEN: &[&str] = &[r"(/\)(o,,,o)(/\)", r"    \\   //"];

const OWL: &[&str] = &[r"  ,_,", r" (O,O)", r" (   )", r#" -"-"-"#];
const OWL_WINK_RIGHT: &[&str] = &[r"   ,_,", r"  (o,O)", r"  (   )", r#"  -"-"-"#];
const OWL_WINK_LEFT: &[&str] = &[r" ,_,", r"(O,o)", r"(   )", r#"-"-"-"#];

const BUNNY_DOWN: &[&str] = &["", r"(\(\", r"( ^.^)", r#"o_(")(")"#];
const BUNNY_UP: &[&str] = &[r"(\(\", r"( ^o^)", r#"o_(")(")"#, ""];

pub const ANIMALS: &[Animal] = &[
    Animal {
        name: "cat",
        poses: &[CAT_LEFT, CAT_UP, CAT_RIGHT, CAT_UP],
    },
    Animal {
        name: "frog",
        poses: &[FROG_SIT, FROG_HOP],
    },
    Animal {
        name: "penguin",
        poses: &[PENGUIN_RIGHT, PENGUIN_LEFT],
    },
    Animal {
        name: "chick",
        poses: &[CHICK_RIGHT, CHICK_LEFT],
    },
    Animal {
        name: "crab",
        poses: &[CRAB_CLOSED, CRAB_OPEN],
    },
    Animal {
        name: "owl",
        poses: &[OWL, OWL_WINK_RIGHT, OWL, OWL_WINK_LEFT],
    },
    Animal {
        name: "bunny",
        poses: &[BUNNY_DOWN, BUNNY_UP],
    },
];

/// A dance playing: which animal, and how far in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dance {
    animal: usize,
    frame: u16,
}

impl Dance {
    pub fn animal(&self) -> &'static Animal {
        &ANIMALS[self.animal]
    }

    /// The pose shown now, with its number in the dance (for its colour).
    pub fn pose(&self) -> (usize, &'static [&'static str]) {
        let step = usize::from(self.frame / POSE_FRAMES);
        let poses = self.animal().poses;
        (step, poses[step % poses.len()])
    }

    /// The next frame; `false` once the dance is over.
    pub fn advance(&mut self) -> bool {
        self.frame += 1;
        self.frame < DANCE_FRAMES
    }
}

/// Picks the animals: at random, never the same one twice in a row.
#[derive(Debug)]
pub struct Dancers {
    /// An xorshift generator's state, never 0.
    state: u64,
    last: Option<usize>,
}

impl Default for Dancers {
    fn default() -> Self {
        Dancers::seeded(0)
    }
}

impl Dancers {
    pub fn seeded(seed: u64) -> Self {
        Dancers {
            state: seed | 1,
            last: None,
        }
    }

    pub fn pick(&mut self) -> Dance {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        let count = ANIMALS.len() as u64;
        let pick = |n: u64| usize::try_from(x % n).expect("below the animal count");
        let animal = match self.last {
            // One of the others: a step of 1 to count - 1 from the last.
            Some(last) => (last + 1 + pick(count - 1)) % ANIMALS.len(),
            None => pick(count),
        };
        self.last = Some(animal);
        Dance { animal, frame: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_animal_is_ascii_art_in_a_small_box() {
        for animal in ANIMALS {
            assert!(animal.poses.len() >= 2, "{} doesn't move", animal.name);
            for line in animal.poses.iter().flat_map(|pose| pose.iter()) {
                assert!(line.is_ascii(), "{}: {line:?}", animal.name);
            }
            let (width, height) = animal.size();
            assert!(
                width <= 20 && height <= 6,
                "{}: {width}×{height}",
                animal.name
            );
        }
    }

    #[test]
    fn two_dances_in_a_row_are_different_animals() {
        for seed in 0..100 {
            let mut dancers = Dancers::seeded(seed);
            let mut last = dancers.pick();
            for _ in 0..20 {
                let next = dancers.pick();
                assert_ne!(next.animal().name, last.animal().name, "seed {seed}");
                last = next;
            }
        }
    }

    #[test]
    fn every_animal_gets_to_dance() {
        let mut dancers = Dancers::seeded(42);
        let names: std::collections::HashSet<_> =
            (0..200).map(|_| dancers.pick().animal().name).collect();

        assert_eq!(names.len(), ANIMALS.len());
    }

    #[test]
    fn a_dance_lasts_three_seconds_of_frames() {
        let mut dance = Dancers::default().pick();

        let frames = std::iter::from_fn(|| dance.advance().then_some(())).count();

        assert_eq!(frames + 1, usize::from(DANCE_FRAMES));
        assert_eq!(u32::from(DANCE_FRAMES) * 40, 3000);
    }

    #[test]
    fn the_pose_changes_every_200_ms() {
        let mut dance = Dancers::default().pick();
        let (first, _) = dance.pose();

        for _ in 1..POSE_FRAMES {
            dance.advance();
        }
        assert_eq!(dance.pose().0, first);
        dance.advance();
        assert_eq!(dance.pose().0, first + 1);
        assert_eq!(u32::from(POSE_FRAMES) * 40, 200);
    }
}
