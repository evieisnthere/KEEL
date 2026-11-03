# Stormworks CAD --- Project Plan

> **Working title:** Stormworks CAD\
> **Project type:** Standalone desktop application\
> **Status:** Initial project blueprint --- architecture and
> implementation details must be validated against real Stormworks
> vehicle files and the installed game version.\
> **Primary goal:** Build a powerful, approachable naval- and
> vehicle-design environment that exports editable, playable Stormworks
> vehicles.

------------------------------------------------------------------------

## 1. Product vision

Stormworks CAD is an independent desktop CAD and systems-engineering
application for designing vehicles for the base game *Stormworks: Build
and Rescue*.

The modelling experience should feel like **Maxsurf and Rhino had a
baby**, with a second, fully supported block-by-block workflow
comparable to the in-game editor. Users should be able to move freely
between precise native block construction and higher-level surface/solid
modelling.

The application is not a Workshop browser, Workshop downloader, mod, or
replacement game. Workshop vehicles are an initial source of sample
files for reverse-engineering and testing. Users work with files they
provide or obtain themselves.

### Core promise

A user should be able to:

1.  Design a hull, superstructure, deck arrangement, cabin and interior
    at real-world scale.
2.  Choose whether to construct it manually block-by-block or generate
    structural blocks from smooth surfaces and CAD geometry.
3.  Add and configure native Stormworks components, paint, decals,
    windows, doors and other supported parts.
4.  Design electrical, logic, composite, mechanical, fluid and
    propulsion systems using visual connections.
5.  Use generators and engineering tools to create or optimise parts of
    the vehicle.
6.  Inspect, validate and customise the generated result down to
    individual Stormworks blocks.
7.  Export a native vehicle file that opens in the game and behaves as
    expected.

The system should be ambitious and extensible. The MVP is a useful
foundation, not the final product.

------------------------------------------------------------------------

## 2. Design principles

-   **Native output first:** Stormworks compatibility is the acceptance
    criterion. CAD geometry is an authoring representation, not a
    substitute for valid game parts.
-   **Two equal modelling workflows:** block-by-block and
    surface/generator workflows must coexist. Neither should become a
    second-class export path.
-   **Everything remains editable:** generated geometry and systems
    should resolve into inspectable, individually editable native parts
    wherever the game format permits.
-   **Real-world units, native game grid:** work in metres and
    millimetres, while making the 25 cm game-block grid explicit and
    easy to use.
-   **Engineering tools are advisory:** calculated performance is an
    estimate unless verified by the game. Never present a simplified
    model as an exact simulation.
-   **Non-destructive design:** retain source geometry, generator
    parameters and feature history so users can revise a hull without
    starting over.
-   **Discover before assuming:** do not invent XML fields, component
    types, connection semantics or game constraints. Derive them from
    supplied files, game assets/code where legally and technically
    available, and in-game validation.
-   **Incremental vertical slices:** prove import → edit → export → load
    in game early, before building a large CAD feature set.

------------------------------------------------------------------------

## 3. Scale, grid and spatial conventions

### 3.1 Units

-   Primary design unit: metres.
-   Secondary display units: millimetres and centimetres.
-   Native structural grid: **0.25 m per block**.
-   Provide a visible grid, snapping controls and coordinate readouts.
-   Make the distinction between continuous CAD geometry and quantised
    game geometry obvious.

The exact mapping between CAD axes, Stormworks axes, vehicle origin,
block centres, rotations and XML coordinates must be established from
reference vehicles. Store it in one tested coordinate-conversion module;
do not scatter conversion logic throughout the application.

### 3.2 Deck and room planning

Provide a ship-layout mode with deck elevations, room boundaries,
bulkheads, compartments, stairs/ladders and openings.

A useful starting convention for ordinary rooms is:

-   1 block (0.25 m) for the floor structure.
-   7--8 blocks (1.75--2.0 m) of clear room height.
-   1 block (0.25 m) for the roof/deck structure.

This is a design guideline, not a hard-coded rule. Allow custom deck
heights, floor thicknesses, headroom and structural offsets.

### 3.3 Coordinate and precision requirements

-   Support decimal CAD coordinates internally; quantise only when
    generating game geometry.
-   Show the nearest block-grid position and quantisation error.
-   Support grid, vertex, edge, face, midpoint and guide snapping.
-   Support local and global coordinate systems.
-   Provide orthographic views, perspective view, section views and a
    dimensioned plan/elevation view.
-   Preserve deterministic transforms and export ordering where
    possible.

------------------------------------------------------------------------

## 4. Main user workflows

### Workflow A --- Native block construction

A user creates or imports a vehicle and places native blocks directly.

Required capabilities:

-   Place, select, move, rotate, duplicate, mirror and delete blocks.
-   Multi-selection, groups, layers and named assemblies.
-   Grid snapping and alignment tools.
-   Box selection, lasso selection and selection filters.
-   Component search and favourites.
-   Copy/paste and repeat placement.
-   Property editing and batch editing.
-   Paint and material assignment.
-   Connection editing.
-   Undo/redo and a useful command history.
-   Precise numerical transforms.
-   Inspect the underlying native block/component data.

This mode should feel substantially faster and more precise than the
in-game editor without hiding the underlying game grid.

### Workflow B --- CAD surfaces and generated geometry

A user creates smooth geometry, then converts it into native game
construction.

Required capabilities:

-   2D sketches and profiles.
-   Lines, arcs, splines and control-point curves.
-   Editable control points and handles.
-   Loft between cross-sections.
-   Sweep along a path.
-   Extrude, revolve, offset and mirror.
-   Surface patch creation and joining.
-   Fillets and chamfers where supported by the geometry kernel.
-   Boolean operations for solid workflows.
-   Surface continuity tools and curvature visualisation.
-   Section curves and waterline inspection.
-   Hull and superstructure surface tools.
-   Cabin and deck layout tools.
-   Editable feature tree/history.

The user should be able to define a hull using stations, keel line,
sheer line, chine lines, transom, bow profile and other
naval-architecture controls, or build it more freely using Rhino-like
curves and surfaces.

### Workflow C --- Generator-assisted construction

Generators create a useful first pass, but users can inspect and modify
every generated result.

Examples:

-   Hull generator.
-   Deck and superstructure generator.
-   Cabin and interior layout generator.
-   Window and door placement tools.
-   Bulkhead and compartment generator.
-   Stairs and ladder generator.
-   Propulsion and engine-room layout assistant.
-   Control-panel and instrument-layout generator.
-   Paint and livery helpers.
-   Hull optimiser.

Every generator must expose its inputs, preview its result, explain
constraints and preserve a route back to its parameters. Generated
native blocks must remain individually editable. If a user manually
edits generated output, the application should warn when a later
regeneration could overwrite those edits and offer a safe
regenerate/merge workflow.

------------------------------------------------------------------------

## 5. Hull and naval-architecture workspace

Hull design is a flagship feature, not merely a generic mesh editor.

### 5.1 Hull creation

Support several starting approaches:

-   Station-based hull definition.
-   Bow, stern, keel and sheer curves.
-   Cross-section sketches.
-   Lofted surfaces through user-defined sections.
-   Freeform surface modelling.
-   Primitive hull templates that can be edited parametrically.

Hull presets should be starting points only, not opaque one-click
shapes. Potential categories include displacement, semi-displacement,
planing, deep-V, flat-bottom, catamaran, trimaran, pontoon and workboat
hulls.

### 5.2 Hull analysis

Display, where sufficient geometry and assumptions exist:

-   Length overall and waterline length.
-   Beam, draft and depth.
-   Displacement estimates at a specified waterline.
-   Approximate wetted surface area.
-   Cross-sectional areas and volume.
-   Longitudinal centre of buoyancy.
-   Approximate centre of buoyancy.
-   Waterline and section curves.
-   Block coefficient and other useful hull-form coefficients.
-   Estimated hull speed and speed/drag indicators.

Every result must show the assumptions, units and
confidence/limitations. Users should be able to choose a loading
condition or waterline.

### 5.3 Hull optimiser

Create an optimiser that can compare candidate hull forms against
user-defined objectives, for example:

-   Target displacement or payload.
-   Length and beam constraints.
-   Draft limit.
-   Desired speed regime.
-   Stability or reserve-buoyancy considerations.
-   Construction complexity.
-   Internal volume.
-   Drag proxy or estimated resistance.

The first version should be a transparent parametric search/optimisation
tool, not a claim of full computational fluid dynamics. Keep the
objective function and constraints visible. Allow users to compare
candidate hulls and choose one to apply.

### 5.4 Accuracy and game reality

Real-world naval-architecture estimates and Stormworks' own physics may
differ. Maintain separate outputs for: - **Real-world estimate:** based
on documented naval-architecture approximations. - **Stormworks
estimate:** based on reverse-engineered game behaviour where possible. -
**In-game result:** measured by a user test or controlled test harness.

Never silently conflate the three.

------------------------------------------------------------------------

## 6. Structural conversion and block generation

The CAD-to-game conversion pipeline is one of the most technically
important subsystems.

### 6.1 Requirements

-   Convert supported closed surfaces/solids into a set of valid native
    structural blocks.
-   Respect the 0.25 m grid.
-   Handle concave shapes, thin surfaces, openings and intersecting
    parts.
-   Avoid gaps, overlaps and invalid internal geometry where possible.
-   Identify areas that cannot be represented exactly.
-   Offer previews before committing generated blocks.
-   Preserve a link from generated blocks back to their source CAD
    feature.
-   Permit per-block manual changes after generation.
-   Support regeneration with conflict detection.

### 6.2 Conversion strategies to investigate

Do not commit to one algorithm before testing it against actual
Stormworks geometry and XML examples.

Investigate:

-   Voxelisation/grid occupancy.
-   Surface-to-block fitting.
-   Boundary extraction and shell generation.
-   Face/triangle coverage.
-   Special handling for wedges, slopes, corners and other non-cubic
    structural pieces.
-   Hybrid conversion: use native block shapes where possible and
    appropriate game-supported geometry for remaining surfaces.

The converter must understand the actual catalogue of structural shapes
and their orientation rules. Do not assume every CAD volume can be
represented exactly.

### 6.3 Conversion diagnostics

Provide a dedicated report containing:

-   Unsupported or unrepresentable geometry.
-   Quantisation error.
-   Missing faces or holes.
-   Overlaps and collisions.
-   Invalid or duplicate native parts.
-   Parts outside supported coordinate or dimension limits.
-   Estimated block count and complexity.
-   Areas where the generated approximation differs from the source
    surface.

Allow users to inspect problem areas directly in the viewport.

------------------------------------------------------------------------

## 7. Native components, systems and connections

Create a data-driven component library derived from the game files and
verified reference vehicles.

### 7.1 Component catalogue

Aim for broad base-game coverage, including:

-   Structural blocks and special structural shapes.
-   Windows, doors, hatches and other openings.
-   Paintable blocks, paint blocks and decals.
-   Seats, beds, ladders, stairs and fittings.
-   Engines and modular engine parts.
-   Gearboxes, clutches, shafts, props and water jets.
-   Fuel, air, exhaust, coolant and fluid components.
-   Batteries, generators, electric motors and electrical components.
-   Logic components, sensors and switches.
-   Microcontrollers.
-   Instrument panels and displays.
-   Composite inputs/outputs and composite-capable components.
-   Throttles, levers, dials, buttons and keypads.
-   Weapons and specialist equipment only to the extent required for
    legitimate base-game vehicle authoring and supported by the file
    format.

The catalogue must be generated from discovered game data where
possible, rather than maintained as a hand-written list of guessed
identifiers.

### 7.2 Component definitions

Each component definition should describe, where applicable:

-   Native identifier and XML representation.
-   Display name, category, icon and documentation.
-   Dimensions, placement rules and rotation constraints.
-   Editable properties and valid ranges.
-   Input/output connection types.
-   Connector positions and directions.
-   Composite channel information.
-   Fluid, electrical, mechanical or logic semantics.
-   Paint and visual behaviour.
-   Dependencies and special configuration data.
-   Known game-version compatibility.

Keep the catalogue versioned and separate from UI code.

### 7.3 Visual connection editor

Support a dedicated systems view as well as connections in the 3D
viewport.

Connection categories may include:

-   Electrical power.
-   Logic on/off and numeric signals.
-   Composite.
-   Mechanical/drive.
-   Fluid.
-   Other connection types discovered in the actual game format.

Features:

-   Drag connector-to-connector wiring.
-   Highlight compatible endpoints.
-   Prevent or warn about incompatible connections.
-   Route and tidy connection lines visually.
-   Label and colour-code networks.
-   Trace a signal or mechanical path.
-   Find unconnected required ports.
-   Detect suspicious loops, missing inputs and disconnected systems.
-   Support multi-connection components and composite channels.
-   Keep logical connections independent of their visual layout.

Do not assume connection types or validation rules until confirmed from
game data.

### 7.4 Instrumentation and operator controls

Create reusable tools for building operator interfaces:

-   Instrument-panel placement and configuration.
-   Composite input/output mapping.
-   Display and monitor setup.
-   Button, toggle, keypad and dial placement.
-   Throttle and lever configuration.
-   Label and control grouping.
-   Signal assignment and channel mapping.
-   Preview of the operator's control layout.

Where possible, provide a control-panel designer that lets users arrange
controls visually, then maps them to native components and connections.

------------------------------------------------------------------------

## 8. Engine builder and propulsion engineering

The engine builder should be a specialised workspace that can create,
configure and analyse propulsion systems.

### 8.1 Engine construction

Support discovered base-game engine architectures, potentially
including:

-   Modular engines.
-   Prefabricated engines.
-   Electric propulsion.
-   Gearboxes and transmissions.
-   Shafts, propellers and water jets.
-   Fuel and supporting systems.
-   Air intake, exhaust and cooling circuits.

The exact scope depends on the actual base-game component and connection
model.

### 8.2 Engine configuration

Provide:

-   Component placement and orientation.
-   Engine layout templates.
-   Fluid and air routing.
-   Cooling and exhaust routing.
-   Fuel-system layout.
-   Gearbox and drive configuration.
-   Clutch and throttle control.
-   Electrical starting and charging.
-   Logic and monitoring connections.
-   Validation of missing or incompatible connections.

### 8.3 Calculation engine

Investigate and extract relevant calculation logic from available game
code/assets where legally and technically accessible. The project should
document exactly what was extracted, what was independently implemented,
and what is only an approximation.

Potential outputs:

-   Estimated torque and power curves.
-   RPM and load behaviour.
-   Fuel consumption.
-   Cooling demand.
-   Gear ratio and output-speed calculations.
-   Propeller/jet matching indicators.
-   Estimated acceleration and top speed under stated assumptions.

Keep calculation logic in a separate testable library. Do not embed
physics calculations in UI code.

### 8.4 Engine test and validation

-   Compare estimates with in-game test results.
-   Store test conditions and vehicle configurations.
-   Allow users to record measured performance.
-   Track prediction error over time.
-   Clearly label unverified models.

------------------------------------------------------------------------

## 9. Paint, decals and visual finish

Paint and decals are core authoring features and must be included in the
data-discovery work.

-   Discover paint-related component types and XML data.
-   Discover decal assets, identifiers, placement and rotation rules.
-   Provide a material/paint browser with previews.
-   Support colour selection and reusable palettes.
-   Support paint-block placement and configuration.
-   Support decals, mirroring, scaling and rotation where the game
    permits.
-   Provide livery layers or groups for organising visual design.
-   Preserve native paint/decal data on import and export.
-   Validate that exported paint and decals render correctly in-game.

The system should distinguish between CAD viewport materials and actual
native Stormworks paint/decal components.

------------------------------------------------------------------------

## 10. Import, reverse engineering and file-format research

This project begins with a deliberate investigation of the game's native
vehicle representation.

### 10.1 Research inputs

Use user-provided example vehicle XML files and relevant game
installation files/assets. A small, diverse reference set is more useful
than a large pile of similar vehicles.

Suggested sample coverage:

-   Simple block-only vehicle.
-   Smooth or unusual hull.
-   Vehicle with windows and doors.
-   Painted vehicle with decals.
-   Vehicle with a microcontroller.
-   Composite instruments and operator controls.
-   Modular engine and gearbox.
-   Electrical and fluid systems.
-   Multi-body or complex assemblies, if supported.

Workshop files are research samples only. Do not build a Steam Workshop
integration or automated downloader.

### 10.2 Reverse-engineering deliverables

Before building the full editor, produce:

1.  A documented inventory of relevant game files and assets.
2.  A vehicle XML structure map.
3.  A catalogue of observed element types and attributes.
4.  Coordinate, rotation and origin conventions.
5.  Component property schemas.
6.  Connection representation and endpoint rules.
7.  Paint/decal representation.
8.  Window and special-geometry representation.
9.  A list of unknowns and confidence levels.
10. Minimal test fixtures for every confirmed feature.

Record which facts were observed in files, inferred from patterns, or
verified in-game. Keep example files and extracted metadata organised,
with provenance and game version.

### 10.3 Parser and writer

Build a robust, round-trip-capable import/export library.

-   Parse real vehicle files without losing unknown fields.
-   Preserve unsupported data when editing supported parts.
-   Keep source XML or a lossless representation for unmodified
    sections.
-   Write deterministic, well-formed output.
-   Validate required fields and references.
-   Report unsupported or malformed structures.
-   Avoid destructive normalisation of data the application does not
    understand.
-   Include round-trip tests: import → export → import should preserve
    the supported and preserved data.

The application should not claim full compatibility until representative
vehicles have passed in-game tests.

------------------------------------------------------------------------

## 11. Application structure and user experience

### 11.1 Primary workspaces

Proposed workspace tabs or modes:

1.  **Build** --- block-by-block construction and component placement.
2.  **Surfaces** --- curves, surfaces, solids and feature history.
3.  **Hull** --- stations, hull forms, waterlines and naval analysis.
4.  **Layout** --- decks, rooms, compartments and superstructure.
5.  **Systems** --- electrical, logic, composite, mechanical and fluid
    connections.
6.  **Engine** --- engine construction and propulsion calculations.
7.  **Paint** --- paint, decals and livery work.
8.  **Validation** --- errors, warnings, compatibility and export
    checks.

A central 3D viewport should remain available across workspaces. Use
contextual panels rather than forcing every task into one crowded
interface.

### 11.2 Essential viewport tools

-   Perspective and orthographic cameras.
-   Top, bottom, front, rear, left and right views.
-   Orbit, pan, zoom and fit selection.
-   Section/clipping planes.
-   Grid and axis controls.
-   Object, face, edge and vertex selection as appropriate.
-   Visibility, isolation and lock.
-   Layers, groups and assemblies.
-   Measurement and dimension tools.
-   Transform gizmos and numeric entry.
-   Undo/redo.
-   Context-sensitive tooltips and command search.

### 11.3 Project organisation

Use a project format that stores:

-   Native vehicle data.
-   CAD geometry and feature history.
-   Generator parameters.
-   Component catalogue version.
-   Design metadata.
-   Validation results.
-   References to external assets where appropriate.
-   Export settings and target game version.

The project format should be versioned and migratable. Saving a project
must not depend on the game being installed.

------------------------------------------------------------------------

## 12. Recommended technical architecture

The exact stack should be proven with a short technical spike before
committing. CAD kernels, bindings and deployment complexity are
significant risks.

### Preferred starting direction

**C++ desktop application with Qt 6 and Open CASCADE Technology (OCCT)**
is the leading candidate because this project needs a serious geometric
modelling kernel, desktop-grade UI and precise 3D interaction.

Potential responsibilities:

  -----------------------------------------------------------------------
  Layer                               Responsibility
  ----------------------------------- -----------------------------------
  Desktop UI                          Qt 6 application shell, docks,
                                      panels, menus, command system

  Geometry kernel                     OCCT curves, surfaces, solids,
                                      topology and geometric operations

  Viewport                            OCCT visualisation initially;
                                      evaluate a separate renderer only
                                      if required

  Native vehicle model                Game-independent in-memory
                                      representation of blocks,
                                      components, connections and
                                      metadata

  File I/O                            XML parser/writer, asset readers
                                      and project serialization

  Geometry conversion                 CAD-to-grid/block conversion and
                                      diagnostics

  Component catalogue                 Versioned, data-driven component
                                      definitions

  Systems graph                       Typed nodes, ports, edges and
                                      validation

  Engineering                         Independent hull, engine and
                                      performance calculation libraries

  Tests                               Unit, integration, golden-file,
                                      geometry and in-game validation
                                      suites
  -----------------------------------------------------------------------

### Architecture rules

-   Keep the native vehicle model independent of Qt and the geometry
    kernel.
-   Keep XML parsing/writing independent of the UI.
-   Keep calculations independent of the UI.
-   Treat the component catalogue as data, not UI conditionals.
-   Use stable IDs for project entities and explicit mappings to native
    game identifiers.
-   Make geometry conversion a separate, testable pipeline.
-   Keep game-version-specific behaviour behind adapters or versioned
    schemas.
-   Use a plugin-friendly architecture for future generators and
    specialised tools, without prematurely building a complicated plugin
    framework.

### Technical spike / stack gate

Before full implementation, prototype:

1.  Loading and displaying an OCCT solid and editable curves.
2.  A responsive Qt viewport with selection and transforms.
3.  Parsing a real vehicle XML file.
4.  Displaying its blocks/components at the correct positions.
5.  Writing a minimally edited file that loads in Stormworks.

If this stack proves impractical, evaluate alternatives based on
measured results rather than preference. Do not build a custom geometry
kernel.

------------------------------------------------------------------------

## 13. Development phases and milestones

The sequence below is intentional. Each phase should produce a usable,
testable result.

### Phase 0 --- Research and feasibility

**Goal:** Understand the native format and prove the core technical
approach.

Tasks: - Collect and organise a diverse set of user-supplied vehicle
files. - Inspect relevant game assets and accessible code. - Document
the vehicle XML structure. - Identify the coordinate system and grid
conventions. - Identify block/component, connection, paint, decal and
window representations. - Prototype XML parsing and writing. - Prototype
geometry-kernel and viewport integration. - Create a minimal in-game
load/export test.

**Exit criteria:** - A reference vehicle can be parsed and visualised in
a prototype. - A harmless edit can be exported and loaded in-game. -
Major unknowns and technical risks are documented. - The proposed stack
has passed a working proof of concept.

### Phase 1 --- Native vehicle foundation

**Goal:** Build the core application and a reliable native vehicle
model.

Tasks: - Desktop application shell and project management. - 3D viewport
and camera controls. - Native XML importer/exporter. - Coordinate
conversion and grid snapping. - Basic block rendering. - Selection,
transforms, duplication and deletion. - Undo/redo and save/load. -
Initial component catalogue. - Validation and diagnostics framework.

**Exit criteria:** - Open a reference vehicle, inspect it, make basic
edits, save and export it. - Unknown XML data survives a round trip. -
Exported vehicles load in-game.

### Phase 2 --- Practical block editor

**Goal:** Make the application useful for real block-by-block
construction.

Tasks: - Placement and rotation tools. - Multi-selection and groups. -
Component search and properties. - Copy/paste, mirror and alignment. -
Layers and visibility. - Paint-block support. - Basic decals and windows
as confirmed by research. - Connection editing for the first confirmed
connection types. - Better selection and editing performance on larger
vehicles.

**Exit criteria:** - Build a simple complete vehicle without opening the
in-game editor. - Export and load it in-game with its supported
components intact.

### Phase 3 --- CAD geometry foundation

**Goal:** Add the Rhino/Maxsurf-style modelling environment.

Tasks: - Sketches, curves and control points. - Surface and solid
representation. - Loft, sweep, extrude and revolve. - Feature tree and
editable parameters. - Surface inspection and section tools. -
CAD-to-grid conversion prototype. - Conversion preview and error
reporting.

**Exit criteria:** - Create a curved hull or superstructure from CAD
geometry. - Convert it to native game geometry. - Inspect and manually
edit the generated blocks. - Export a valid vehicle with documented
approximation limits.

### Phase 4 --- Hull design and layout tools

**Goal:** Make vessel design a first-class workflow.

Tasks: - Station-based hull designer. - Hull templates and parameter
editing. - Waterline and section analysis. - Deck and superstructure
tools. - Deck-height and room-layout planning. - Bulkhead and
compartment generation. - Window, door and opening tools. - Hull metrics
and initial speed/displacement estimates.

**Exit criteria:** - Design a multi-deck vessel with a generated hull,
superstructure and basic interior layout. - Generated output is editable
and passes geometry diagnostics.

### Phase 5 --- Systems editor

**Goal:** Make complex vehicles practical to wire and configure.

Tasks: - Typed connection graph. - Visual connection editor. - Logic and
electrical components. - Composite channels and instrument panels. -
Buttons, dials, throttles and operator controls. - Microcontroller
import/configuration support as discovered. - Connection validation and
tracing. - Systems templates and reusable subassemblies.

**Exit criteria:** - Build and validate a representative control
system. - Export connections and configurations correctly. - Verify
behaviour in-game.

### Phase 6 --- Engine builder and engineering tools

**Goal:** Build propulsion systems and provide useful engineering
estimates.

Tasks: - Engine and drivetrain component workflows. -
Fluid/air/cooling/exhaust routing. - Gearbox and propulsor
configuration. - Engine templates and system validation. -
Extract/replicate relevant game calculations where possible. - Engine
performance estimates and charts. - Hull speed and resistance
estimates. - Hull optimiser with transparent constraints/objectives. -
In-game comparison and calibration workflow.

**Exit criteria:** - Construct a representative engine/drivetrain
system. - Produce documented estimates with assumptions. - Compare
predictions with measured in-game results.

### Phase 7 --- Finish, compatibility and advanced workflows

**Goal:** Move from capable editor to dependable daily-use CAD suite.

Tasks: - Broader component catalogue coverage. - Advanced paint and
decal tooling. - Improved generator editing and regeneration/merge. -
Large-vehicle performance optimisation. - Better error recovery and
autosave. - Project format migration. - Accessibility and UI polish. -
Documentation, tutorials and example projects. - Compatibility testing
across supported game versions.

**Exit criteria:** - A substantial range of real vehicles can be
imported, edited and exported. - Core authoring workflows are documented
and reliable. - Unsupported features are preserved or clearly reported
rather than silently discarded.

------------------------------------------------------------------------

## 14. Testing and quality strategy

Testing must be built into the project from the beginning.

### 14.1 Automated tests

-   XML parser and writer tests.
-   Round-trip preservation tests.
-   Coordinate and rotation conversion tests.
-   Component schema validation.
-   Connection compatibility tests.
-   Geometry operation tests.
-   CAD-to-grid conversion tests.
-   Hull calculation tests against known examples.
-   Engine calculation tests against known cases.
-   Project save/load and migration tests.
-   Large-vehicle performance tests.

### 14.2 Golden reference fixtures

For each representative input vehicle:

-   Keep an unmodified source fixture.
-   Record expected parsed structure.
-   Record known component counts and connection counts.
-   Record expected geometry bounds.
-   Record expected import/export behaviour.
-   Compare semantic XML rather than relying only on textual formatting.

### 14.3 In-game validation

The game is the ultimate compatibility test.

For every significant native feature: 1. Create or import a small test
vehicle. 2. Export it. 3. Load it in Stormworks. 4. Check visual
appearance and component placement. 5. Test functional behaviour where
applicable. 6. Record game version, test conditions and result.

Maintain a compatibility matrix so a feature is marked as verified,
partially verified, experimental or unsupported.

------------------------------------------------------------------------

## 15. Non-functional requirements

-   Stable on large vehicles and complex CAD surfaces.
-   No silent loss of vehicle data.
-   Autosave and crash recovery.
-   Clear, actionable validation errors.
-   Responsive viewport and background processing for expensive
    operations.
-   Deterministic project saving and export where practical.
-   Versioned project files and migration support.
-   No requirement for an internet connection during normal design work.
-   No dependency on Steam Workshop access.
-   User-controlled access to game installation files.
-   Keep licensing and redistribution restrictions in mind when using
    extracted game assets or code. Do not bundle proprietary game assets
    or copied game code without confirming the rights to do so.

------------------------------------------------------------------------

## 16. Risks and mitigation

  -----------------------------------------------------------------------
  Risk                    Why it matters          Mitigation
  ----------------------- ----------------------- -----------------------
  Incomplete XML          Incorrect exports can   Research first;
  understanding           break vehicles          preserve unknown
                                                  fields; use in-game
                                                  fixtures

  CAD surfaces do not map Smooth geometry may     Prototype conversion
  cleanly to game blocks  require approximations  early; show
                                                  quantisation and error
                                                  diagnostics

  Geometry-kernel         Can consume the project Run an early stack
  integration complexity  before useful features  spike; use an
                          exist                   established kernel

  Game physics are only   Estimates may be        Separate real-world,
  partially understood    misleading              reverse-engineered and
                                                  measured results

  Component catalogue     Game updates may change Versioned data
  becomes stale           definitions             extraction and
                                                  compatibility tests

  Generated geometry is   Users lose control      Stable provenance links
  difficult to edit       after generation        and individual
                                                  native-part editing

  Large vehicle           Viewport and graph      Profile real fixtures;
  performance             operations may become   use spatial indexing
                          slow                    and background tasks

  Scope becomes           Too many advanced       Enforce phase exit
  unmanageable            features compete for    criteria and maintain a
                          attention               prioritised backlog
  -----------------------------------------------------------------------

------------------------------------------------------------------------

## 17. Initial repository structure

Proposed starting layout; refine after the technical spike.

``` text
stormworks-cad/
├── PROJECT_PLAN.md
├── README.md
├── CONTRIBUTING.md
├── docs/
│   ├── architecture/
│   ├── reverse-engineering/
│   ├── file-format/
│   ├── geometry/
│   ├── engineering/
│   ├── testing/
│   └── decisions/
├── research/
│   ├── reference-vehicles/
│   ├── extracted-metadata/
│   └── findings/
├── src/
│   ├── app/
│   ├── ui/
│   ├── viewport/
│   ├── geometry/
│   ├── vehicle-model/
│   ├── vehicle-io/
│   ├── component-catalogue/
│   ├── connections/
│   ├── conversion/
│   ├── hull-tools/
│   ├── engine-tools/
│   ├── generators/
│   └── validation/
├── tests/
│   ├── unit/
│   ├── integration/
│   ├── fixtures/
│   └── in-game/
└── examples/
```

Do not create every directory as empty scaffolding. Add modules when
their first real implementation begins.

------------------------------------------------------------------------

## 18. Claude Code working instructions

Claude Code should treat this document as the product direction and
maintain the project as an engineering codebase, not a disposable
prototype.

### Required behaviour

1.  **Research before implementing native formats.** Inspect real
    examples and document evidence. Never invent XML attributes,
    component IDs, connection types or game rules.
2.  **Start with Phase 0.** Do not begin by building a large UI full of
    mock components.
3.  **Prove vertical slices.** Prioritise a working parse → render →
    edit → export → in-game test loop.
4.  **Keep architecture boundaries.** UI, geometry, native vehicle
    model, file I/O, conversion and calculations must remain separate.
5.  **Write tests with features.** A feature is not complete merely
    because it appears to work in the viewport.
6.  **Preserve unknown data.** If the application cannot interpret a
    field, do not discard it during import/export.
7.  **Document discoveries.** Add findings to
    `docs/reverse-engineering/` and update schemas/fixtures when
    evidence changes.
8.  **Avoid premature abstraction.** Build the smallest robust
    implementation that supports the next milestone.
9.  **Do not fake completion.** Clearly distinguish implemented,
    stubbed, experimental and verified features.
10. **Keep the plan current.** Update milestone status, known
    limitations, decisions and next tasks as work progresses.
11. **Ask for input when necessary.** Request additional representative
    vehicle files or game-version details when a feature cannot be
    understood from available evidence.
12. **Do not add Workshop integration.** User-provided files are
    research fixtures; Workshop downloading/browsing is outside scope.

### Definition of done

A feature is complete only when: - Its intended behaviour is
documented. - Its data model and implementation are in the correct
architectural layer. - It has appropriate automated tests. - It handles
invalid or unsupported input safely. - It has been checked in the UI
where relevant. - Native output has been validated in-game where the
feature affects exported vehicles. - Known limitations are documented.

------------------------------------------------------------------------

## 19. First tasks for the project

Claude Code should begin with these tasks, in order:

1.  Inspect the development environment and identify available
    compilers, Qt versions, OCCT availability and packaging constraints.
2.  Create a short architecture decision record comparing Qt 6 + OCCT
    with realistic alternatives.
3.  Request and inventory the first batch of reference vehicle files.
4.  Inspect those files and produce an initial XML structure report.
5.  Identify the smallest vehicle file that can serve as a round-trip
    fixture.
6.  Implement a minimal parser that preserves unknown data.
7.  Implement a minimal native vehicle model and coordinate conversion
    tests.
8.  Prototype displaying the reference vehicle in a 3D viewport.
9.  Implement a no-op or harmless round-trip export.
10. Load that export in Stormworks and document the result.
11. Only after this works, begin the full block editor.

### First milestone

**A reference Stormworks vehicle opens in Stormworks CAD, appears in the
correct position and orientation in a 3D viewport, can be saved back out
without losing data, and the exported file still opens in Stormworks.**

That is the foundation on which every ambitious feature in this plan
depends.

---

## 20. Product identity and visual direction

### Product name

**KEEL — Stormworks Engineering Suite**

Use KEEL as the primary product identity. The full name should appear in the application title/about screen, installer and documentation where useful.

The visual identity is a professional naval-engineering environment:

- Deep navy blue application background.
- Blue-grey panels and tool surfaces.
- Cyan selection, active-tool and measurement highlights.
- White technical linework and labels.
- Orange warnings, errors requiring attention and important construction indicators.
- Subtle nautical-chart, station-line and blueprint influences.

Avoid excessive decorative nautical imagery. The interface should feel like credible professional engineering software, not a themed game UI.

### Interface principles

- Make the first-use experience approachable without removing advanced capability.
- Prefer progressive disclosure: common operations are easy to find; advanced parameters are available when needed.
- Use clear tool names, contextual help, sensible defaults and short guided workflows.
- Provide beginner-friendly generators and templates alongside precise manual controls.
- Keep the viewport visually dominant.
- Use consistent colour semantics across the 3D viewport, connection editor, diagnostics and engineering tools.
- Provide light/dark accessibility considerations only if they can be implemented without compromising the primary visual identity.

---

## 21. Native `.keel` project format

Use `.keel` as KEEL's native editable project extension.

The `.keel` file must preserve more than the exported Stormworks vehicle XML. It is the complete working design, including editable CAD features, source geometry, generator settings and application-specific metadata.

### Recommended container format

Start with a versioned ZIP-based project container using the `.keel` extension. This is straightforward to inspect, test, recover and migrate, while allowing large assets to be stored separately inside the archive.

Proposed internal layout:

```text
example-vessel.keel
├── manifest.json
├── project/
│   ├── vehicle-model.json
│   ├── cad-model.json
│   ├── feature-history.json
│   ├── generators.json
│   └── metadata.json
├── assets/
│   ├── imported/
│   ├── decals/
│   └── previews/
├── catalogue/
│   └── target-game-version.json
├── exports/
│   └── vehicle.xml
└── thumbnails/
    └── preview.png
```

This is a logical layout, not a requirement that every file must exist in every project.

### Format requirements

- Include a manifest with format version, application version, project ID, units, coordinate conventions and target game version.
- Preserve editable feature history and generator parameters.
- Store the native Stormworks vehicle model separately from CAD source geometry.
- Store imported source files or a managed reference/copy according to explicit user settings.
- Support compression and large assets.
- Make saving atomic where practical, with backup/recovery support.
- Validate archives before opening them.
- Support format migrations and clear incompatibility messages.
- Never make a `.keel` project dependent on a particular absolute filesystem path.
- Provide a recovery strategy for interrupted saves and damaged archives.
- Keep exported Stormworks files as ordinary game-compatible files; `.keel` is the editable KEEL project, not a replacement for the game's vehicle format.

Do not over-engineer the container before the first useful project exists. Prove create → save → close → reopen → edit → save, then formalise migrations.

---

## 22. Importing external CAD geometry

KEEL should support importing common engineering geometry formats, with a clear distinction between editable solid/surface models and triangle meshes.

### STEP

Support STEP import, prioritising AP203 and AP214 compatibility, and investigate AP242 where practical.

Expected uses:
- Import designed hulls, fittings, machinery, components and assemblies from other CAD packages.
- Preserve assembly structure and part names where available.
- Import solids and surfaces into the geometry kernel.
- Allow users to position, scale only with explicit confirmation, orient and inspect imported geometry.
- Provide options to keep imported geometry as reference, convert it into editable KEEL geometry, or use it as a source for native Stormworks block generation.

STEP export is a possible later feature, but STEP import is the initial requirement.

### STL

Support binary and ASCII STL import.

Expected uses:
- Import reference meshes and externally generated hull forms.
- Inspect mesh bounds, normals, connected components and topology.
- Detect holes, non-manifold edges, self-intersections where feasible and inconsistent normals.
- Offer mesh repair tools with a preview and report.
- Convert suitable meshes into surfaces/solids only where the geometry kernel can do so reliably.
- Use meshes as reference geometry or as input to the block-generation pipeline.

An STL is a triangle mesh, not a parametric CAD model. Do not imply that importing an STL automatically recovers its original sketches, feature history or exact NURBS surfaces.

### Import diagnostics

For every imported file:
- Show units and inferred scale; ask for confirmation when ambiguous.
- Show bounds and orientation before committing.
- Report unsupported entities or geometry.
- Preserve the original source asset inside the project or at a managed reference location.
- Avoid silently changing scale, orientation or topology.

Additional formats such as IGES or OBJ can be considered later based on actual user need.

---

## 23. Block-first viewport and surface overlays

### Core visual rule

**The vehicle should always be previewed as Stormworks blocks.**

The main viewport must communicate what the exported in-game construction will actually look like, rather than presenting a smooth CAD model as if it were directly usable by Stormworks.

When CAD surfaces or solids exist:
- Show the generated/associated Stormworks block representation as the primary, opaque viewport model.
- Show source CAD surfaces as an optional translucent overlay.
- Allow overlay opacity, visibility and colour to be adjusted.
- Make the relationship between source surface and generated blocks inspectable.
- Highlight differences between ideal CAD geometry and quantised native geometry.
- Allow users to isolate source geometry, generated blocks or both for diagnosis, but return to the block-first default.
- Display unsupported or unconverted regions clearly.

The overlay is a design reference. It must not obscure the native block result or imply that the game renders the original smooth CAD surface.

### Preview states

Provide clear viewport states for:
1. **Native blocks:** the normal default view.
2. **Blocks + CAD overlay:** native blocks with translucent source surfaces.
3. **CAD source only:** a temporary modelling/inspection view.
4. **Conversion diagnostics:** colour-coded gaps, overlaps, unsupported areas and quantisation error.
5. **Systems overlay:** connections and system paths over the block model.

The default on opening a project should be Native blocks or Blocks + CAD overlay if a source surface is actively being edited.

---

## 24. Multihull design, symmetry and recursive assemblies

KEEL must support advanced vessel configurations, including catamarans and trimarans, without making these workflows inaccessible to new users.

### 24.1 Multihull design

Provide dedicated helpers for:
- Catamaran hull pairs.
- Trimarans with a central hull and outriggers.
- User-defined multi-hull arrangements.
- Adjustable hull spacing, relative position, orientation and vertical offset.
- Shared deck and bridge structures.
- Cross-deck beams and connecting structure.
- Independent hull editing and analysis.
- Symmetry and alignment checks.
- Combined vehicle bounds and waterline inspection.

Do not assume every multihull is perfectly symmetric. Allow independent editing after generation and clearly indicate when symmetry constraints have been broken.

### 24.2 Recursive symmetry

Support symmetry not only for individual blocks, but for assemblies and nested groups.

Examples:
- Mirror a hull and all its components to create the second catamaran hull.
- Mirror an assembly containing rooms, structural blocks, windows and systems.
- Create trimaran outriggers from a master hull assembly.
- Apply symmetry to nested subassemblies while retaining their internal relationships.
- Repeat assemblies around an axis or along a pattern where appropriate.

Requirements:
- Define symmetry planes/axes and local coordinate frames.
- Support nested and recursive symmetry operations.
- Preserve or deliberately remap component orientation and connection endpoints.
- Handle mirrored handedness for asymmetric geometry and components.
- Provide preview-before-commit.
- Allow linked symmetry, where edits propagate, and independent copies, where they do not.
- Detect unsupported mirrored components or connection types.
- Ensure the resulting native block positions and rotations are valid.

Recursive symmetry must be a first-class assembly capability, not a one-off catamaran button.

### 24.3 New-user workflow

Provide a guided multihull generator:
1. Choose catamaran, trimaran or custom multihull.
2. Define the master hull or select an existing hull.
3. Set hull count, spacing, offsets and orientation.
4. Choose linked or independent hull editing.
5. Generate connecting deck/support geometry.
6. Preview the native blocks and CAD overlays.
7. Validate geometry and symmetry.
8. Commit the assembly, with every generated part remaining editable.

Advanced users must also be able to build the same result manually from assemblies, transforms and symmetry tools.

---

## 25. Watertightness and hull integrity

Watertightness is a critical design and validation concern. KEEL should help users produce hulls that are geometrically closed and appropriately constructed for the game's actual flooding and buoyancy behaviour.

### 25.1 Distinguish different meanings

Do not treat these as interchangeable:

- **CAD watertightness:** a closed, manifold geometric shell with no unintended boundary edges or holes.
- **Structural closure:** the generated native blocks form the intended enclosed hull structure.
- **Stormworks flood integrity:** the vehicle behaves as sealed in the game, according to the game's actual compartment, block and flooding rules.

A mathematically closed CAD surface does not automatically prove that the exported Stormworks vehicle is floodproof.

### 25.2 Geometry validation

For CAD solids/shells:
- Detect open edges and boundary loops.
- Detect non-manifold edges and vertices.
- Detect self-intersections where feasible.
- Check face orientation and normals.
- Identify disconnected shells and unintended gaps.
- Report thin or degenerate regions.
- Show holes and problem regions in the viewport.
- Provide repair suggestions and safe, previewed repair operations.

### 25.3 Native block integrity

For generated block structures:
- Detect missing boundary coverage and unintended openings.
- Detect gaps between structural parts.
- Identify intersections and invalid overlaps.
- Check hull/deck/bulkhead joins.
- Identify windows, doors, hatches and other openings as intentional penetrations requiring appropriate treatment.
- Distinguish intended openings from accidental holes.
- Provide compartment and bulkhead inspection tools where the game format/physics supports meaningful analysis.

### 25.4 Flood-integrity checks

Develop a game-specific validation layer based on observed and tested Stormworks behaviour.

- Establish test fixtures for sealed and deliberately breached hulls.
- Determine which native part types and arrangements affect flooding.
- Identify likely leak paths and unsealed boundaries.
- Provide a compartment-oriented visualisation if the game model supports it.
- Label heuristic checks as warnings, not guarantees.
- Where possible, validate with controlled in-game tests.

A vehicle should not receive an unconditional “watertight” pass solely because its CAD shell is closed. Use precise statuses such as “CAD shell closed”, “native structure checks passed” and “in-game flood test verified”.

---

## 26. Additional implementation priorities

The following requirements should be reflected in the roadmap and backlog:

- Add `.keel` project save/load to the native vehicle foundation phase.
- Add STEP and STL import to the CAD geometry foundation phase.
- Make block-first preview a foundational viewport requirement, not a later visual polish task.
- Add source-surface overlays and conversion diagnostics alongside CAD-to-grid conversion.
- Add recursive symmetry and assembly transforms to the practical block editor / assembly foundation.
- Add catamaran and trimaran workflows to the hull/layout phase.
- Add CAD watertightness checks during the geometry phase.
- Add native structural and in-game flood-integrity checks as reverse-engineering and validation mature.
- Include imported geometry, project versioning and provenance in save/load tests.

These requirements are part of the product direction and should not be treated as optional stretch goals.

## 27. Systems generators and vehicle systems engineering

KEEL must generate complete, interconnected vehicle systems rather than treating engines, electrical equipment, controls and fittings as isolated catalogue objects. Generators should create editable native Stormworks components, connections, layout objects and configuration data. Every generated system must remain inspectable and manually editable.

### 27.1 Electrical distribution and power management generator

Provide an electrical-system workspace that can generate and validate practical vessel electrical installations, including:

- Main and auxiliary distribution panels.
- Circuit breakers, fuses and individually protected circuits where supported by the game.
- Batteries, battery banks, isolation switches and charging connections.
- Battery state/voltage gauges and other electrical instrumentation.
- Alternators and engine-driven electrical generators.
- Backup batteries, emergency power supplies and automatic/manual changeover arrangements.
- Busbars, distribution branches, switches, relays and contactors where represented by native components.
- Lighting circuits, navigation lights, interior/exterior lighting and emergency lighting.
- Load groups, circuit labels, cable/connection routing and panel schedules.
- Power budgeting, estimated current draw, reserve capacity and battery endurance.

The generator should support sensible defaults and guided setup for beginners, while allowing experienced users to define circuit topology, priorities, protection and switching logic. It should identify overloads, unpowered components, missing returns/grounds where applicable, inadequate reserve capacity, and backup systems that cannot actually supply their intended loads.

Where the game does not model a real-world electrical concept, KEEL must not imply that it does. Clearly distinguish game-supported behaviour from optional engineering annotations and estimates.

### 27.2 Advanced microcontroller and automation editor

Build a substantially more capable microcontroller authoring environment than the game's built-in editor, while preserving compatibility with the game's actual microcontroller format and execution model.

It should offer three synchronised representations:

1. **Graphical logic editor:** visual nodes, gates, arithmetic, comparisons, timers, memory, signal routing and supported composite operations.
2. **Readable text representation:** a code-like, plaintext view of the same logic, suitable for inspection, editing, searching, commenting and version control.
3. **Native-compatible microcontroller output:** export to the game's own graphical Lua-based microcontroller system and native vehicle data.

The text representation must not silently become a separate incompatible programming language. Define a canonical intermediate representation (IR) so graphical nodes, text and native output can be translated and validated consistently. If a construct cannot be represented faithfully in both directions, mark the conversion as lossy and explain exactly what will be changed.

Key capabilities:

- Nested functions/subassemblies or reusable logic modules where the game format permits.
- Named inputs and outputs, types, units and signal descriptions.
- Composite signal packing/unpacking and channel management.
- Search, comments, labels, grouping, reusable templates and copy/paste.
- Simulation/test inputs, live signal inspection and step-through debugging where feasible.
- Cycle/tick awareness, execution-cost warnings and detection of feedback loops or invalid connections.
- Import and round-trip editing of existing workshop microcontrollers.
- Starter templates for engine control, generator regulation, battery management, bilge pumps, lighting, navigation, alarms, ballast and helm control.

The editor must be grounded in the native game's supported logic blocks, Lua behaviour, signal types and XML structure. Do not invent unsupported nodes or assume ordinary desktop Lua can be executed unchanged by the game.

### 27.3 Modular engine generator

Provide a modular-engine design workspace that builds the engine from actual game components and connections. It should support:

- Cylinder count, arrangement, placement and engine dimensions.
- Manifolds and the relevant intake, exhaust, fuel and cooling connections.
- Air intakes, filters or other game-supported intake components.
- Fuel supply, pumps and fuel routing where applicable.
- Cooling loops, pumps, radiators, heat exchangers and coolant plumbing.
- Exhaust routing and back-pressure-related constraints if the game models them.
- Starters, ignition, throttle and engine-control logic.
- Engine-driven electrical generators and accessory loads.
- Clutches, gearboxes, gear ratios, shafts, propellers, waterjets and other supported propulsion connections.
- Sensors and instrumentation for RPM, temperature, pressure, fuel and system status.
- Editable generated layouts, connection diagrams and component schedules.

The generator should allow users to define a target duty (for example, displacement/size constraints, target speed, vessel mass, propulsion type, desired cruise speed and operating profile) and propose a practical configuration. It must expose assumptions and let users change every generated component.

### 27.4 Powertrain simulation and performance estimation

KEEL should attempt to simulate the engine and drivetrain under load, rather than simply comparing a headline engine power figure with an estimated requirement. This is a major engineering feature and should be developed in stages.

Model, where reliable game-specific equations or measured behaviour can be established:

- Engine torque and power across RPM and throttle/load.
- Engine response, usable RPM range and any relevant thermal or efficiency limits.
- Gearbox ratios, losses and supported shifting behaviour.
- Clutch engagement and slip.
- Shaft/drivetrain losses and propulsion loading.
- Propeller or waterjet load characteristics, as supported by evidence.
- Vessel resistance and speed estimates, including hull form and displacement effects.
- Auxiliary electrical and mechanical loads where they affect available power.
- Fuel consumption across operating points and resulting endurance/range.

The simulator should solve for a consistent operating point under load where possible. It should report whether the engine can reach and sustain the requested operating condition, not merely whether its theoretical peak output exceeds a rough demand estimate.

Use an evidence hierarchy:
1. Equations or behaviour confirmed from accessible game source/data.
2. Controlled in-game measurements and repeatable experiments.
3. Published or community-tested empirical approximations, clearly labelled.
4. General naval/mechanical engineering approximations, explicitly marked as estimates.

Do not present an approximate model as an exact Stormworks simulation. Every result should include confidence/quality indicators, assumptions, model limitations and a way to compare predictions against in-game test results. Provide sensitivity ranges when input uncertainty materially changes the result.

Outputs should include estimated shaft power, torque, RPM, speed, fuel burn, endurance and range, plus warnings for an overloaded engine, unsuitable gearing, poor operating point, excessive auxiliary demand or inadequate reserve. Range estimates must account for usable fuel, expected consumption and the chosen operating profile rather than assuming maximum tank capacity is usable.

### 27.5 Fuel storage, transfer and ventilation generator

Generate complete fuel installations, including:

- Fuel tanks of configurable size and shape, with native-compatible construction.
- Tank placement, fill level and usable-capacity calculations.
- Fill, feed, return, transfer and isolation arrangements where supported.
- Pumps, valves, manifolds, filters and engine feed routing.
- Gas relief/vent valves and vent routing, represented using actual game-supported parts or clearly labelled design annotations.
- Tank level sensors, low-fuel alarms, gauges and control logic.
- Multiple tank selection, balancing and reserve-tank arrangements.
- Fuel-system diagrams, labels and service access checks.

The generator should distinguish game mechanics from real-world safety conventions. It must not claim that a generated in-game vent or relief arrangement provides real-world fuel-system safety certification.

### 27.6 Cooling-system generator

Generate complete engine and equipment cooling circuits, including:

- Coolant tanks/reservoirs where supported.
- Pumps, radiators, heat exchangers and cooling components.
- Intake and outlet routing, branches and multiple cooling loops.
- Component-to-loop assignment and flow direction.
- Temperature sensors, gauges, alarms and automatic fan/pump control.
- Redundancy or emergency cooling arrangements where practical.
- Pipe routing, connection validation, service access and component schedules.

Estimate heat rejection and cooling capacity only to the extent supported by game-specific equations or measured data. Flag uncertain or unmodelled behaviour rather than fabricating precision.

### 27.7 Helm, bridge and operator-station generator

Provide a guided generator for a coherent helm/bridge or control station. Users should be able to choose vessel type, control philosophy, available panel space and desired instruments, then customise the result.

Potential generated equipment and functions include:

- Helm/wheel, rudder and steering controls.
- Throttle, reverse, gear and clutch controls.
- Engine start/stop and engine status.
- Battery, generator and electrical distribution monitoring.
- Fuel level, tank selection and low-fuel warnings.
- Cooling temperature, pump status and alarms.
- Bilge pump controls and bilge level warnings.
- Ballast tank selection, fill/drain pumps, valves, level indicators and trim controls.
- Navigation lights and other lighting controls.
- Compass, heading, speed, depth and other game-supported navigation instruments.
- GPS/map display or waypoint/navigation interfaces where supported.
- Radar and other sensor displays where supported by native components.
- Anchoring, doors, hatches, pumps, deck machinery and auxiliary equipment.
- Alarm acknowledgement, warning lamps, status panels and emergency controls.
- Labels, legends, sensible grouping and consistent instrument placement.

Provide beginner-friendly presets (small launch, workboat, patrol vessel, yacht, ferry, cargo vessel, multihull) but keep all controls and generated logic editable. The generator should produce the required microcontroller logic and connections as well as the physical panel layout.

### 27.8 Ballast, trim and stability-control generator

Support ballast and trim systems as a first-class vessel subsystem:

- Tank placement and capacity definition.
- Fill/drain pumps, valves and pipe routing.
- Port/starboard and fore/aft balancing.
- Manual, assisted and automatic ballast modes.
- Tank level sensing and helm display/control integration.
- List, trim and draft estimates based on the vessel's mass model.
- Interlocks and warnings for conflicting valve/pump states.
- Optional automatic levelling or trim-hold control using supported sensors and logic.

Stability calculations must identify their assumptions and limitations. A simplified mass-balance estimate must not be presented as a complete hydrostatic stability assessment.

### 27.9 Lighting, navigation and vessel services generators

Include reusable generators for common vessel systems:

- Navigation and signal lighting with configurable vessel presets.
- Interior, exterior, deck, search and emergency lighting circuits.
- Bilge pumps, high-water alarms and compartment monitoring.
- Freshwater, greywater and other service systems where useful and supported.
- Ventilation fans and temperature-controlled spaces.
- Doors, hatches, ramps, winches, cranes and deck machinery.
- Fire detection/suppression-style game systems where native components support them.
- Mooring/anchor equipment and control stations.
- Sensor networks and distributed status/alert panels.

Generators should share the same component catalogue, connection model, microcontroller editor and electrical load model rather than implementing separate incompatible subsystems.

### 27.10 Shared generator framework

All generators should use a common framework:

- **Inputs:** vessel dimensions, design intent, constraints, selected components and user preferences.
- **Preview:** show the proposed physical layout, connections, panel arrangement and system diagram before committing.
- **Validation:** identify missing components, incompatible ports, disconnected networks, capacity issues, inaccessible service points and spatial conflicts.
- **Commit:** create editable native blocks/components, connections, microcontrollers and metadata.
- **Regenerate:** allow users to update a generated system when its inputs change, while preserving manual edits where possible.
- **Explain:** provide a readable bill of materials, connection schedule, assumptions and a summary of design decisions.
- **Test:** run graph checks and available simulations before export.

Generators must never be opaque one-click black boxes. Users should be able to inspect, modify, partially regenerate and take ownership of every generated result.

### 27.11 Recommended implementation order

Build these capabilities incrementally rather than attempting every simulation at once:

1. Establish the native component/port/connection catalogue and common generator framework.
2. Build the microcontroller IR, graphical editor, readable text view and native export/import round-trip.
3. Build basic electrical distribution, battery and lighting generators.
4. Build fuel and cooling system generators with connection validation.
5. Build modular-engine layout and drivetrain generation.
6. Implement a measured, game-specific engine/drivetrain model and compare it against in-game tests.
7. Add helm/bridge panels, navigation, alarms, bilge and ballast controls.
8. Expand into advanced automation, redundancy, multihull-specific systems and more detailed performance analysis.

Do not block the entire application on a perfect physics simulator. Deliver useful, honest engineering estimates early, then improve them as the native game mechanics are understood.
