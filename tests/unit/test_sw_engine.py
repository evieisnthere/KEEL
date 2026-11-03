import math, os, sys
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "sim-reference"))
import sw_engine as e


def test_rps_efficiency_curve():
    assert e.rps_efficiency(0) == 1.0
    assert math.isclose(e.rps_efficiency(50), 0.5)
    assert e.rps_efficiency(125) == 0.0


def test_afr_window_moves_with_temperature():
    assert e.afr_window(0) == (16.0, 12.0)
    assert e.afr_window(100) == (20.0, 10.0)


def test_rev_limiter_cuts_air():
    s = e.cylinder_tick(61, 1.0, 1.0, 50, 1000, 10, 100)
    assert s.air == 0 and s.energy == 0


def test_no_firing_below_2_rps():
    s = e.cylinder_tick(1.5, 1.0, 0.1, 50, 1000, 10, 100)
    assert not s.fires and s.d_rps == 0


def test_coupling_conserves_momentum():
    a, b = e.couple(10, 2, 0, 6, 1.0)
    assert math.isclose(2 * 10 + 6 * 0, 2 * a + 6 * b) and math.isclose(a, b)
