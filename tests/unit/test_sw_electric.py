import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..', 'sim-reference'))
import sw_electric as E


def test_no_battery_no_power():
    n = E.Network(); n.begin_tick(); n.add(5.0)
    assert n.remove(1.0) == 0.0


def test_battery_pool_conserves_charge_without_load():
    b = E.Battery(1600, 0.5); n = E.Network([b])
    for _ in range(100):
        n.begin_tick(); n.end_tick()
    assert abs(b.frac - 0.5) < 1e-6


def test_batteries_equalise():
    a, b = E.Battery(1600, 1.0), E.Battery(1600, 0.0); n = E.Network([a, b])
    for _ in range(5000):
        n.begin_tick(); n.end_tick()
    assert abs(a.frac - b.frac) < 1e-3 and abs(a.frac + b.frac - 1.0) < 1e-6


def test_motor_spins_up_and_drains():
    b = E.Battery(1600, 1.0); n = E.Network([b]); w = 0.0
    for _ in range(600):
        n.begin_tick(); w = E.motor_tick(w, 1.0, 1.2, 2, n); n.end_tick()
    assert 10 < w < 20 and b.frac < 1.0


def test_generator_cap():
    n = E.Network([E.Battery(1600, 0.0)]); n.begin_tick()
    w, p = E.generator_tick(200.0, 0.85, 125, n)
    assert abs(w - 150.0) < 1e-9 and abs(p - 0.85 * 125 * 0.1 * 200 * 0.25) < 1e-9
