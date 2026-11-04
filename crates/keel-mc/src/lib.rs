//! KEEL microcontroller toolkit: the component table, a lossless graph view of a `microprocessor_definition`
//! (RE-0013) that edits positions, wires, properties and IO nodes while preserving everything else, and a tick
//! simulator following the game's latch-then-compute order.
pub mod expr;
pub mod graph;
pub mod sim;

/// Signal kinds, numbered as the vehicle's logic link `type` (and an IO node's `type`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Sig {
    Bool,
    Num,
    Comp,
    Video,
    Audio,
}

impl Sig {
    pub fn link_kind(self) -> i32 {
        match self {
            Sig::Bool => 0,
            Sig::Num => 1,
            Sig::Comp => 5,
            Sig::Video => 6,
            Sig::Audio => 7,
        }
    }
    pub fn from_link_kind(k: i32) -> Sig {
        match k {
            1 => Sig::Num,
            5 => Sig::Comp,
            6 => Sig::Video,
            7 => Sig::Audio,
            _ => Sig::Bool,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Sig::Bool => "on/off",
            Sig::Num => "number",
            Sig::Comp => "composite",
            Sig::Video => "video",
            Sig::Audio => "audio",
        }
    }
    pub const ALL: [Sig; 5] = [Sig::Bool, Sig::Num, Sig::Comp, Sig::Video, Sig::Audio];
    /// Bridge component type for an IO node of this kind (input = outside → inside).
    pub fn bridge_type(self, input: bool) -> i32 {
        let base = match self {
            Sig::Bool => 0,
            Sig::Num => 2,
            Sig::Comp => 4,
            Sig::Video => 6,
            Sig::Audio => 8,
        };
        base + if input { 0 } else { 1 }
    }
    pub fn from_bridge_type(t: i32) -> (Sig, bool) {
        let s = [Sig::Bool, Sig::Num, Sig::Comp, Sig::Video, Sig::Audio][(t.clamp(0, 9) / 2) as usize];
        (s, t % 2 == 0)
    }
}

/// How a property is stored on the component's `object` element.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Store {
    /// attribute on `object`
    Attr,
    /// child element with `text` (as typed) and `value` (parsed, omitted when 0)
    Val,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PropKind {
    Number,
    Text,
    /// multi-line source text
    Code,
}

#[derive(Clone, Copy, Debug)]
pub struct Prop {
    pub key: &'static str,
    pub label: &'static str,
    pub store: Store,
    pub kind: PropKind,
}

const fn p(key: &'static str, label: &'static str, store: Store, kind: PropKind) -> Prop {
    Prop { key, label, store, kind }
}

#[derive(Clone, Debug)]
pub struct CompType {
    pub id: i32,
    pub name: &'static str,
    pub group: &'static str,
    pub about: &'static str,
    pub inputs: Vec<(String, Sig)>,
    pub outputs: Vec<(String, Sig)>,
    pub props: Vec<Prop>,
}

pub const GROUPS: [&str; 8] = ["Logic", "Arithmetic", "Functions", "Memory", "Control", "Composite", "Properties", "Other"];

use PropKind::*;
use Sig::*;
use Store::*;

/// The component table (ids are the `type` attribute; 60 types, RE-0013 §2). `count` is the configured channel count
/// for the 32-channel composite writers (their inputs depend on it).
pub fn comp_type(id: i32, count: usize) -> Option<CompType> {
    let b2 = || vec![("A", Bool), ("B", Bool)];
    let n2 = || vec![("A", Num), ("B", Num)];
    let expr = p("e", "Function", Attr, Text);
    let (name, group, about, inputs, outputs, props): (&str, &str, &str, Vec<(&str, Sig)>, Vec<(&str, Sig)>, Vec<Prop>) = match id {
        0 => ("NOT", "Logic", "Inverts its input.", vec![("A", Bool)], vec![("NOT A", Bool)], vec![]),
        1 => ("AND", "Logic", "On when both inputs are on.", b2(), vec![("A AND B", Bool)], vec![]),
        2 => ("OR", "Logic", "On when either input is on.", b2(), vec![("A OR B", Bool)], vec![]),
        3 => ("XOR", "Logic", "On when exactly one input is on.", b2(), vec![("A XOR B", Bool)], vec![]),
        4 => ("NAND", "Logic", "Off only when both inputs are on.", b2(), vec![("A NAND B", Bool)], vec![]),
        5 => ("NOR", "Logic", "On only when both inputs are off.", b2(), vec![("A NOR B", Bool)], vec![]),
        6 => ("Add", "Arithmetic", "A + B.", n2(), vec![("A + B", Num)], vec![]),
        7 => ("Subtract", "Arithmetic", "A − B.", n2(), vec![("A - B", Num)], vec![]),
        8 => ("Multiply", "Arithmetic", "A × B.", n2(), vec![("A x B", Num)], vec![]),
        9 => ("Divide", "Arithmetic", "A ÷ B; 0 and a flag when B is (near) zero.", n2(), vec![("A / B", Num), ("Divide By Zero", Bool)], vec![]),
        10 => ("f(x, y, z)", "Functions", "Evaluates a function of three numbers.", vec![("x", Num), ("y", Num), ("z", Num)], vec![("f(x, y, z)", Num)], vec![expr]),
        11 => ("Clamp", "Arithmetic", "Limits a number to [min, max].", vec![("Input Number", Num)], vec![("Clamped Input", Num)], vec![p("min", "Min", Val, Number), p("max", "Max", Val, Number)]),
        12 => ("Threshold", "Logic", "On while the input is within [min, max].", vec![("Input Number", Num)], vec![("Within Threshold", Bool)], vec![p("min", "Min", Val, Number), p("max", "Max", Val, Number)]),
        13 => ("Memory Register", "Memory", "Stores a number on Set; Reset restores the reset value.", vec![("Set", Bool), ("Reset", Bool), ("Number to store", Num)], vec![("Stored Number", Num)], vec![p("r", "Reset value", Val, Number)]),
        14 => ("Abs", "Arithmetic", "Absolute value.", vec![("Input Number", Num)], vec![("Absolute Value", Num)], vec![]),
        15 => ("Constant Number", "Arithmetic", "A fixed number.", vec![], vec![("Constant Value", Num)], vec![p("n", "Number", Val, Number)]),
        16 => ("Constant On Signal", "Logic", "Always on.", vec![], vec![("On Signal", Bool)], vec![]),
        17 => ("Greater Than", "Logic", "On when A > B.", n2(), vec![("A > B", Bool)], vec![]),
        18 => ("Less Than", "Logic", "On when A < B.", n2(), vec![("A < B", Bool)], vec![]),
        19 => ("Property Slider", "Properties", "A slider set in the vehicle editor.", vec![], vec![("Slider Value", Num)], vec![p("name", "Name", Attr, Text), p("min", "Min", Val, Number), p("max", "Max", Val, Number), p("int", "Step", Val, Number), p("v", "Value", Val, Number)]),
        20 => ("Property Dropdown", "Properties", "A list of named values chosen in the vehicle editor.", vec![], vec![("Selected Value", Num)], vec![p("name", "Name", Attr, Text), p("i", "Selected index", Attr, Number)]),
        21 => ("Numerical Junction", "Control", "Routes the value to one of two outputs.", vec![("Value To Pass Through", Num), ("Switch Signal", Bool)], vec![("On Path", Num), ("Off Path", Num)], vec![]),
        22 => ("Numerical Switchbox", "Control", "Chooses between two numbers.", vec![("First Value (On)", Num), ("Second Value (Off)", Num), ("Switch Signal", Bool)], vec![("Switched Value", Num)], vec![]),
        23 => ("PID Controller", "Control", "Per-tick PID loop.", vec![("Setpoint", Num), ("Process Variable", Num), ("Active", Bool)], vec![("Control Output", Num)], vec![p("kp", "P gain", Val, Number), p("ki", "I gain", Val, Number), p("kd", "D gain", Val, Number)]),
        24 => ("SR Latch", "Memory", "Set turns on, Reset turns off.", vec![("Set", Bool), ("Reset", Bool)], vec![("Output", Bool), ("NOT Output", Bool)], vec![]),
        25 => ("JK Flip Flop", "Memory", "Like an SR latch; both inputs toggle.", vec![("Set", Bool), ("Reset", Bool)], vec![("Output", Bool), ("NOT Output", Bool)], vec![]),
        26 => ("Capacitor", "Control", "Delays switching on and off.", vec![("Charge", Bool)], vec![("Stored Charge", Bool)], vec![p("ct", "Charge time (s)", Attr, Number), p("dt", "Discharge time (s)", Attr, Number)]),
        27 => ("Blinker", "Control", "Flashes while its control is on.", vec![("Control Signal", Bool)], vec![("Blinking Signal", Bool)], vec![p("on", "On time (s)", Attr, Number), p("off", "Off time (s)", Attr, Number)]),
        28 => ("Push To Toggle", "Memory", "Toggles on each rising edge.", vec![("Toggle Signal", Bool)], vec![("Internal State", Bool)], vec![]),
        29 => ("Composite Read (on/off)", "Composite", "Reads an on/off channel.", vec![("Composite Signal", Comp), ("Start Channel", Num)], vec![("Read Value", Bool)], vec![p("i", "Channel (0 = from input)", Attr, Number)]),
        30 => ("Composite Write (on/off)", "Composite", "Writes an on/off channel.", vec![("Composite Signal", Comp), ("Set Value", Bool)], vec![("Modified Composite Signal", Comp)], vec![p("i", "Channel", Attr, Number)]),
        31 => ("Composite Read (number)", "Composite", "Reads a number channel.", vec![("Composite Signal", Comp), ("Start Channel", Num)], vec![("Read Value", Num)], vec![p("i", "Channel (0 = from input)", Attr, Number)]),
        32 => ("Composite Write (number)", "Composite", "Writes a number channel.", vec![("Composite Signal", Comp), ("Set Value", Num)], vec![("Modified Composite Signal", Comp)], vec![p("i", "Channel", Attr, Number)]),
        33 => ("Property Toggle", "Properties", "An on/off switch set in the vehicle editor.", vec![], vec![("Toggle Value", Bool)], vec![p("n", "Name", Attr, Text), p("on", "On label", Attr, Text), p("off", "Off label", Attr, Text), p("v", "Value (true/false)", Attr, Text)]),
        34 => ("Property Number", "Properties", "A number typed in the vehicle editor.", vec![], vec![("Set Number", Num)], vec![p("n", "Name", Attr, Text), p("v", "Value", Val, Number)]),
        35 => ("Delta", "Arithmetic", "Change since the previous tick.", vec![("Input Value", Num)], vec![("Delta Of Input Value", Num)], vec![]),
        36 => ("f(x, y, z, w, a, b, c, d)", "Functions", "Evaluates a function of eight numbers.", ["x", "y", "z", "w", "a", "b", "c", "d"].map(|n| (n, Num)).to_vec(), vec![("f(...)", Num)], vec![expr]),
        37 => ("Up/Down Counter", "Memory", "Counts up and down; mode 1 clamps to [min, max].", vec![("Up", Bool), ("Down", Bool), ("Reset", Bool)], vec![("Value", Num)], vec![p("m", "Clamp (0/1)", Attr, Number), p("r", "Reset value", Val, Number), p("i", "Increment", Val, Number), p("min", "Min", Val, Number), p("max", "Max", Val, Number)]),
        38 => ("Modulo (fmod)", "Arithmetic", "Remainder of A ÷ B.", n2(), vec![("A % B", Num)], vec![]),
        39 => ("PID Controller (Advanced)", "Control", "PID with gains as inputs.", vec![("Setpoint", Num), ("Process Variable", Num), ("Proportional Gain", Num), ("Integral Gain", Num), ("Derivative Gain", Num), ("Active", Bool)], vec![("Control Output", Num)], vec![]),
        40 | 41 => {
            let s = if id == 40 { Num } else { Bool };
            let mut ins = vec![("Composite Signal", Comp)];
            const LABELS: [&str; 32] = [
                "Input 1", "Input 2", "Input 3", "Input 4", "Input 5", "Input 6", "Input 7", "Input 8", "Input 9", "Input 10", "Input 11", "Input 12", "Input 13", "Input 14", "Input 15", "Input 16", "Input 17", "Input 18", "Input 19", "Input 20", "Input 21", "Input 22", "Input 23", "Input 24", "Input 25", "Input 26", "Input 27", "Input 28", "Input 29", "Input 30", "Input 31", "Input 32",
            ];
            ins.extend(LABELS.iter().take(count.clamp(1, 32)).map(|l| (*l, s)));
            (if id == 40 { "Composite Write (number, multi)" } else { "Composite Write (on/off, multi)" }, "Composite", "Writes several channels at once.", ins, vec![("Modified Composite Signal", Comp)], vec![p("count", "Channels", Attr, Number), p("offset", "First channel offset", Attr, Number)])
        }
        42 => ("Equal", "Logic", "On when |A − B| < epsilon.", n2(), vec![("A = B", Bool)], vec![p("e", "Epsilon", Val, Number)]),
        43 => ("Tooltip Number", "Other", "Shows a number when hovering the microcontroller.", vec![("Display Number", Num), ("Is Error", Bool)], vec![], vec![p("l", "Label", Attr, Text), p("m", "Mode", Attr, Number)]),
        44 => ("Tooltip On/Off", "Other", "Shows an on/off state when hovering the microcontroller.", vec![("Display Signal", Bool)], vec![], vec![p("l", "Label", Attr, Text), p("on", "On text", Attr, Text), p("off", "Off text", Attr, Text), p("m", "Mode", Attr, Number)]),
        45 => ("f(x)", "Functions", "Evaluates a function of one number.", vec![("x", Num)], vec![("f(x)", Num)], vec![expr]),
        46 => ("Boolean f(x,y,z,w)", "Functions", "Evaluates a logic expression of four signals.", ["x", "y", "z", "w"].map(|n| (n, Bool)).to_vec(), vec![("f(...)", Bool)], vec![expr]),
        47 => ("Boolean f(x..d)", "Functions", "Evaluates a logic expression of eight signals.", ["x", "y", "z", "w", "a", "b", "c", "d"].map(|n| (n, Bool)).to_vec(), vec![("f(...)", Bool)], vec![expr]),
        48 => ("Pulse", "Control", "One-tick pulse on a change (mode 0 rising, 1 falling, 2 any).", vec![("Toggle Signal", Bool)], vec![("Pulse", Bool)], vec![p("m", "Mode", Attr, Number)]),
        49 => ("Timer (TON)", "Control", "On after enabled for the duration.", vec![("Timer Enable", Bool), ("Duration", Num)], vec![("Timing Complete", Bool)], vec![p("u", "Unit (0 s, 1 ticks)", Attr, Number)]),
        50 => ("Timer (TOF)", "Control", "On while enabled until the duration passes.", vec![("Timer Enable", Bool), ("Duration", Num)], vec![("Timing", Bool)], vec![p("u", "Unit (0 s, 1 ticks)", Attr, Number)]),
        51 => ("Timer (RTO)", "Control", "Retentive on-delay timer.", vec![("Timer Enable", Bool), ("Duration", Num), ("Reset", Bool)], vec![("Timing Complete", Bool)], vec![p("u", "Unit (0 s, 1 ticks)", Attr, Number)]),
        52 => ("Timer (RTF)", "Control", "Retentive off-delay timer.", vec![("Timer Enable", Bool), ("Duration", Num), ("Reset", Bool)], vec![("Timing", Bool)], vec![p("u", "Unit (0 s, 1 ticks)", Attr, Number)]),
        53 => ("Composite Switchbox", "Composite", "Chooses between two composite signals.", vec![("First Signal (On)", Comp), ("Second Signal (Off)", Comp), ("Switch Signal", Bool)], vec![("Switched Signal", Comp)], vec![]),
        54 => ("Number To Composite Binary", "Composite", "Bits of a number to on/off channels.", vec![("Number To Convert", Num)], vec![("Binary Signal", Comp)], vec![]),
        55 => ("Composite Binary To Number", "Composite", "On/off channels to the bits of a number.", vec![("Signal To Convert", Comp)], vec![("Encoded Number", Num)], vec![]),
        56 => ("Lua Script", "Functions", "A Lua script (onTick / onDraw).", vec![("Data Input", Comp), ("Video Input", Video)], vec![("Data Output", Comp), ("Video Output", Video)], vec![p("script", "Script", Attr, Code)]),
        57 => ("Video Switchbox", "Other", "Chooses between two video signals.", vec![("Video 1 (On)", Video), ("Video 2 (Off)", Video), ("Switch Signal", Bool)], vec![("Video Output", Video)], vec![]),
        58 => ("Property Text", "Properties", "A text note shown in the vehicle editor.", vec![], vec![], vec![p("n", "Name", Attr, Text), p("v", "Text", Attr, Text)]),
        59 => ("Audio Switchbox", "Other", "Chooses between two audio signals.", vec![("Audio 1 (On)", Audio), ("Audio 2 (Off)", Audio), ("Switch Signal", Bool)], vec![("Audio Output", Audio)], vec![]),
        _ => return None,
    };
    let own = |v: Vec<(&str, Sig)>| v.into_iter().map(|(l, s)| (l.to_string(), s)).collect();
    Some(CompType { id, name, group, about, inputs: own(inputs), outputs: own(outputs), props })
}

/// All component types (multi-channel writers at their default single channel).
pub fn all_types() -> Vec<CompType> {
    (0..60).filter_map(|i| comp_type(i, 1)).collect()
}

/// The two ends of an IO bridge as a component type: an input bridge has one output, an output bridge one input.
pub fn bridge_type(t: i32, label: &str) -> CompType {
    let (s, input) = Sig::from_bridge_type(t);
    let l = label.to_string();
    CompType {
        id: t,
        name: if input { "Input" } else { "Output" },
        group: "IO",
        about: "Microcontroller IO node",
        inputs: if input { vec![] } else { vec![(l.clone(), s)] },
        outputs: if input { vec![(l, s)] } else { vec![] },
        props: vec![],
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn table_is_complete() {
        assert_eq!(super::all_types().len(), 60);
        assert_eq!(super::comp_type(40, 5).unwrap().inputs.len(), 6);
    }
}
