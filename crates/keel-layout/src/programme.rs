//! Room programmes: which rooms each boat type needs, how big, where they go and how they are lit.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum BoatType {
    CruiseShip,
    RoPaxFerry,
    Workboat,
    Tug,
    MotorYacht,
    PassengerFerry,
    Patrol,
    Rescue,
}

impl BoatType {
    pub const ALL: [BoatType; 8] = [
        BoatType::CruiseShip,
        BoatType::RoPaxFerry,
        BoatType::Workboat,
        BoatType::Tug,
        BoatType::MotorYacht,
        BoatType::PassengerFerry,
        BoatType::Patrol,
        BoatType::Rescue,
    ];
    pub fn name(self) -> &'static str {
        match self {
            BoatType::CruiseShip => "Cruise ship",
            BoatType::RoPaxFerry => "Ro/Pax ferry",
            BoatType::Workboat => "Workboat",
            BoatType::Tug => "Tug",
            BoatType::MotorYacht => "Motor yacht / cruiser",
            BoatType::PassengerFerry => "Passenger ferry",
            BoatType::Patrol => "Patrol boat",
            BoatType::Rescue => "Rescue boat",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum RoomKind {
    Bridge,
    CaptainCabin,
    CrewCabin,
    CrewMess,
    Galley,
    Head,
    EngineRoom,
    Electrical,
    Store,
    Workshop,
    PassengerCabin,
    Suite,
    OwnerCabin,
    GuestCabin,
    Saloon,
    Restaurant,
    Bar,
    Lounge,
    Reception,
    Shop,
    Gym,
    Laundry,
    Medical,
    PassengerSeating,
    CarDeck,
    Armoury,
    SurvivorBay,
}

/// Which deck a room prefers: below the weather deck, on it (main deck interior / superstructure first tier), or up
/// in the superstructure; `Top` is the highest tier (bridge).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeckPref {
    Below,
    Main,
    Upper,
    Top,
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZonePref {
    Fwd,
    Mid,
    Aft,
    Any,
}

impl RoomKind {
    pub fn name(self) -> &'static str {
        use RoomKind::*;
        match self {
            Bridge => "Bridge",
            CaptainCabin => "Captain's cabin",
            CrewCabin => "Crew cabin",
            CrewMess => "Crew mess",
            Galley => "Galley",
            Head => "Head",
            EngineRoom => "Engine room",
            Electrical => "Electrical room",
            Store => "Store",
            Workshop => "Workshop",
            PassengerCabin => "Passenger cabin",
            Suite => "Suite",
            OwnerCabin => "Owner's cabin",
            GuestCabin => "Guest cabin",
            Saloon => "Saloon",
            Restaurant => "Restaurant",
            Bar => "Bar",
            Lounge => "Lounge",
            Reception => "Reception",
            Shop => "Shop",
            Gym => "Gym",
            Laundry => "Laundry",
            Medical => "Medical bay",
            PassengerSeating => "Passenger seating",
            CarDeck => "Car deck",
            Armoury => "Armoury",
            SurvivorBay => "Survivor bay",
        }
    }

    /// Default lighting: colour temperature (None = the ship-wide setting) and brightness. Working spaces are bright
    /// and cooler; living spaces warm and dimmer; the bridge dim for night vision.
    pub fn light(self) -> (Option<f64>, f64) {
        use RoomKind::*;
        match self {
            Bridge => (None, 0.3),
            EngineRoom | Workshop | CarDeck => (Some(5000.0), 1.0),
            Medical => (Some(5500.0), 1.0),
            Galley | Laundry | Electrical => (Some(4000.0), 0.9),
            Head => (None, 0.8),
            Store | Armoury => (None, 0.65),
            Gym | Reception | Shop => (None, 0.75),
            CrewMess | PassengerSeating | SurvivorBay => (None, 0.65),
            Restaurant | Saloon => (None, 0.5),
            Lounge => (None, 0.4),
            Bar => (None, 0.28),
            CrewCabin | CaptainCabin | PassengerCabin | GuestCabin => (None, 0.4),
            Suite | OwnerCabin => (None, 0.35),
        }
    }

    /// Placement priority (0 first): what a boat cannot do without goes in before the extras.
    pub fn priority(self) -> u8 {
        use RoomKind::*;
        match self {
            Bridge | EngineRoom | CarDeck => 0,
            Electrical | CrewCabin | CaptainCabin | OwnerCabin | Head | PassengerSeating | SurvivorBay | Medical => 1,
            Galley | CrewMess | PassengerCabin | GuestCabin | Suite | Saloon | Restaurant => 2,
            _ => 3,
        }
    }

    pub fn is_cabin(self) -> bool {
        use RoomKind::*;
        matches!(self, CaptainCabin | CrewCabin | PassengerCabin | Suite | OwnerCabin | GuestCabin)
    }

    /// Rooms whose lights are worked from the room's own push button (all except open technical spaces).
    pub fn has_switch(self) -> bool {
        !matches!(self, RoomKind::CarDeck)
    }
}

/// One line of a programme: `count` rooms of `kind`, each of at least `area` m².
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomReq {
    pub kind: RoomKind,
    pub count: usize,
    pub area: f64,
    pub deck: DeckPref,
    pub zone: ZonePref,
    /// wants a window or porthole
    pub window: bool,
}

/// Size inputs for a programme.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgrammeInput {
    pub boat: BoatType,
    pub length: f64,
    pub beam: f64,
    /// crew berths (0 = estimate from size)
    pub crew: usize,
    /// passenger berths or seats (0 = estimate from size)
    pub passengers: usize,
}

fn r(kind: RoomKind, count: usize, area: f64, deck: DeckPref, zone: ZonePref, window: bool) -> RoomReq {
    RoomReq { kind, count, area, deck, zone, window }
}

/// Estimated crew and passengers for a size.
pub fn estimate_people(boat: BoatType, length: f64, beam: f64) -> (usize, usize) {
    let a = length * beam;
    let n = |k: f64, lo: usize| ((a * k).round() as usize).max(lo);
    use BoatType::*;
    match boat {
        CruiseShip => (n(0.012, 6), n(0.06, 20)),
        RoPaxFerry => (n(0.008, 4), n(0.08, 20)),
        PassengerFerry => (n(0.006, 2), n(0.25, 12)),
        Workboat => (n(0.03, 2), 0),
        Tug => (n(0.03, 3), 0),
        MotorYacht => (n(0.015, 1), n(0.03, 2)),
        Patrol => (n(0.06, 4), 0),
        Rescue => (n(0.05, 3), n(0.1, 6)),
    }
}

/// The room programme for a boat type and size.
pub fn programme(inp: &ProgrammeInput) -> Vec<RoomReq> {
    use BoatType::*;
    use DeckPref::*;
    use RoomKind::*;
    use ZonePref as Z;
    let (ec, ep) = estimate_people(inp.boat, inp.length, inp.beam);
    let crew = if inp.crew > 0 { inp.crew } else { ec };
    let pax = if inp.passengers > 0 { inp.passengers } else { ep };
    let crew_cabins = crew.saturating_sub(1).div_ceil(2).max(1);
    let heads = |people: usize| people.div_ceil(8).max(1);
    let mut v = vec![r(Bridge, 1, (inp.beam * 2.0).clamp(6.0, 60.0), Top, Z::Fwd, true), r(EngineRoom, 1, (inp.length * inp.beam * 0.12).clamp(6.0, 400.0), Below, Z::Aft, false)];
    match inp.boat {
        CruiseShip => {
            v.extend([
                r(CaptainCabin, 1, 14.0, Upper, Z::Fwd, true),
                r(CrewCabin, crew_cabins, 8.0, Below, Z::Any, false),
                r(CrewMess, 1, (crew as f64 * 1.5).max(10.0), Below, Z::Mid, false),
                r(Galley, 1, (pax as f64 * 0.3).clamp(10.0, 200.0), Main, Z::Mid, false),
                r(Restaurant, 1, (pax as f64 * 1.2).clamp(20.0, 600.0), Main, Z::Aft, true),
                r(Bar, 1, (pax as f64 * 0.3).clamp(12.0, 150.0), Upper, Z::Aft, true),
                r(Lounge, 1, (pax as f64 * 0.5).clamp(15.0, 300.0), Upper, Z::Fwd, true),
                r(Reception, 1, 15.0, Main, Z::Mid, false),
                r(Shop, 1, 12.0, Main, Z::Mid, false),
                r(Gym, 1, 15.0, Upper, Z::Mid, true),
                r(Medical, 1, 10.0, Below, Z::Mid, false),
                r(Laundry, 1, 8.0, Below, Z::Any, false),
                r(Suite, (pax / 20).max(1), 22.0, Upper, Z::Any, true),
                r(PassengerCabin, pax.div_ceil(2), 11.0, Any, Z::Any, true),
                r(Head, heads(crew), 3.0, Below, Z::Any, false),
                r(Store, 2, 10.0, Below, Z::Fwd, false),
                r(Electrical, 1, 6.0, Below, Z::Aft, false),
            ]);
        }
        RoPaxFerry => {
            v.extend([
                r(CarDeck, 1, (inp.length * inp.beam * 0.5).max(40.0), Main, Z::Any, false),
                r(CaptainCabin, 1, 10.0, Upper, Z::Fwd, true),
                r(CrewCabin, crew_cabins, 7.0, Upper, Z::Aft, true),
                r(CrewMess, 1, 10.0, Upper, Z::Aft, false),
                r(Galley, 1, 12.0, Upper, Z::Mid, false),
                r(Restaurant, 1, (pax as f64 * 0.6).clamp(20.0, 300.0), Upper, Z::Mid, true),
                r(PassengerSeating, 1, (pax as f64 * 0.7).clamp(20.0, 400.0), Upper, Z::Fwd, true),
                r(PassengerCabin, pax / 6, 9.0, Upper, Z::Aft, true),
                r(Shop, 1, 10.0, Upper, Z::Mid, false),
                r(Reception, 1, 10.0, Upper, Z::Mid, false),
                r(Head, heads(pax / 3 + crew), 3.0, Upper, Z::Any, false),
                r(Store, 1, 8.0, Below, Z::Fwd, false),
                r(Electrical, 1, 6.0, Below, Z::Aft, false),
            ]);
        }
        Workboat => {
            v.extend([
                r(CrewCabin, crew_cabins, 6.0, Below, Z::Fwd, false),
                r(CrewMess, 1, 8.0, Main, Z::Mid, true),
                r(Galley, 1, 5.0, Main, Z::Mid, false),
                r(Head, 1, 2.5, Any, Z::Any, false),
                r(Workshop, 1, 8.0, Main, Z::Aft, false),
                r(Store, 1, 6.0, Below, Z::Fwd, false),
                r(Electrical, 1, 3.0, Below, Z::Aft, false),
            ]);
        }
        Tug => {
            v.extend([
                r(CaptainCabin, 1, 8.0, Main, Z::Fwd, true),
                r(CrewCabin, crew_cabins, 6.0, Below, Z::Fwd, false),
                r(CrewMess, 1, 9.0, Main, Z::Mid, true),
                r(Galley, 1, 5.0, Main, Z::Mid, false),
                r(Head, 1, 2.5, Main, Z::Any, false),
                r(Store, 1, 6.0, Below, Z::Fwd, false),
                r(Electrical, 1, 3.0, Below, Z::Aft, false),
            ]);
        }
        MotorYacht => {
            v.extend([
                r(OwnerCabin, 1, 14.0, Below, Z::Mid, true),
                r(GuestCabin, pax.saturating_sub(2).div_ceil(2).max(1), 9.0, Below, Z::Fwd, true),
                r(CrewCabin, crew_cabins, 5.0, Below, Z::Aft, false),
                r(Saloon, 1, (inp.length * inp.beam * 0.12).clamp(12.0, 80.0), Main, Z::Aft, true),
                r(Galley, 1, 6.0, Main, Z::Mid, true),
                r(Head, heads(pax + crew).max(2), 3.0, Below, Z::Any, false),
                r(Lounge, 1, 12.0, Upper, Z::Aft, true),
                r(Store, 1, 4.0, Below, Z::Aft, false),
                r(Electrical, 1, 3.0, Below, Z::Aft, false),
            ]);
        }
        PassengerFerry => {
            v.extend([
                r(PassengerSeating, 1, (pax as f64 * 0.6).max(12.0), Main, Z::Any, true),
                r(Bar, 1, (pax as f64 * 0.08).clamp(6.0, 40.0), Main, Z::Aft, true),
                r(Head, heads(pax / 4 + crew), 3.0, Main, Z::Any, false),
                r(CrewMess, 1, 6.0, Below, Z::Mid, false),
                r(Store, 1, 5.0, Below, Z::Fwd, false),
                r(Electrical, 1, 3.0, Below, Z::Aft, false),
            ]);
        }
        Patrol => {
            v.extend([
                r(CaptainCabin, 1, 7.0, Main, Z::Fwd, false),
                r(CrewCabin, crew_cabins, 6.0, Below, Z::Fwd, false),
                r(CrewMess, 1, 10.0, Main, Z::Mid, false),
                r(Galley, 1, 5.0, Main, Z::Mid, false),
                r(Head, heads(crew), 2.5, Below, Z::Any, false),
                r(Armoury, 1, 5.0, Below, Z::Mid, false),
                r(Medical, 1, 5.0, Main, Z::Mid, false),
                r(Store, 1, 5.0, Below, Z::Fwd, false),
                r(Electrical, 1, 3.0, Below, Z::Aft, false),
            ]);
        }
        Rescue => {
            v.extend([
                r(CrewCabin, crew_cabins, 5.0, Below, Z::Fwd, false),
                r(Medical, 1, 8.0, Main, Z::Mid, true),
                r(SurvivorBay, 1, (pax as f64 * 1.0).max(8.0), Main, Z::Aft, true),
                r(Galley, 1, 4.0, Main, Z::Mid, false),
                r(Head, heads(crew + pax / 2), 2.5, Main, Z::Any, false),
                r(Store, 1, 5.0, Below, Z::Fwd, false),
                r(Electrical, 1, 3.0, Below, Z::Aft, false),
            ]);
        }
    }
    v.retain(|q| q.count > 0);
    // the electrical room holds the battery bus and a gate panel (2 × 4 voxels per lighting circuit)
    let circuits: usize = v.iter().map(|q| q.count).sum::<usize>() + 3;
    for q in v.iter_mut().filter(|q| q.kind == Electrical) {
        q.area = q.area.max(2.5 + circuits as f64 * 0.25);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_type_has_bridge_engine_room_and_electrics() {
        for b in BoatType::ALL {
            let p = programme(&ProgrammeInput { boat: b, length: 30.0, beam: 8.0, crew: 0, passengers: 0 });
            for k in [RoomKind::Bridge, RoomKind::EngineRoom, RoomKind::Electrical] {
                assert!(p.iter().any(|q| q.kind == k), "{b:?} lacks {k:?}");
            }
        }
    }

    #[test]
    fn cruise_ship_scales_cabins_with_size() {
        let small = programme(&ProgrammeInput { boat: BoatType::CruiseShip, length: 60.0, beam: 12.0, crew: 0, passengers: 0 });
        let big = programme(&ProgrammeInput { boat: BoatType::CruiseShip, length: 150.0, beam: 24.0, crew: 0, passengers: 0 });
        let cabins = |p: &[RoomReq]| p.iter().filter(|q| q.kind == RoomKind::PassengerCabin).map(|q| q.count).sum::<usize>();
        assert!(cabins(&big) > 3 * cabins(&small));
    }
}
