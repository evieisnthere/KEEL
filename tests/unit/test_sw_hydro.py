import math, os, sys
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "sim-reference"))
import sw_hydro as h


def test_mass_scaling_endpoints():
    assert h.buoyancy_divisor(500) == 10.0
    assert h.buoyancy_divisor(25000) == 20.0
    assert h.buoyancy_divisor(100000) == 20.0
    assert h.water_drag_scale(1000) == 1.0
    assert h.water_drag_scale(25000) == 5.0


def test_single_block_floats_light_vehicle():
    # One 0.25 m block of mass 1: buoyancy if fully submerged = 1000*10*0.015625/10 = 15.625 > weight 10
    assert math.isclose(h.buoyancy_force(0.25 ** 3, 1.0), 15.625)


def test_fill_fraction():
    assert h.cell_fill_fraction(0.0, 0.0, 0.125) == 0.5
    assert h.cell_fill_fraction(1.0, 0.0, 0.125) == 1.0
    assert h.cell_fill_fraction(-1.0, 0.0, 0.125) == 0.0


def test_pressure_face_opposes_motion():
    f = h.face_bin_force((0, 0, 1), 0.0625, (0, 0, 2.0), mass=1e6)
    assert f[2] < 0 and abs(f[0]) < 1e-12 and abs(f[1]) < 1e-12


def test_suction_face_opposes_motion():
    f = h.face_bin_force((0, 0, -1), 0.0625, (0, 0, 2.0), mass=1e6)
    assert f[2] < 0


def test_impulse_cap_binds_for_light_body():
    s, m = 5.0, 10.0
    f = h.face_bin_force((0, 0, 1), 1.0, (0, 0, s), mass=m)
    assert math.isclose(math.sqrt(sum(c * c for c in f)), s * m * 0.5 / 60)
