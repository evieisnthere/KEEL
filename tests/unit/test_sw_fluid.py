import os, sys
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "sim-reference"))
import sw_fluid as f


def test_small_pipe_pressure_is_fill_fraction_times_60():
    v = f.Volume(4.0); v.amounts[0] = 2.0
    assert v.pressure() == 30.0


def test_tank_liquid_pressure_rises_only_near_full():
    v = f.Volume(100.0); v.amounts[1] = 50.0
    low = v.pressure()
    v.amounts[1] = 96.0
    assert low < 1.0 and v.pressure() > 20.0


def test_transfer_flows_downhill_and_conserves():
    a = f.Volume(8.0); a.amounts[6] = 8.0
    b = f.Volume(8.0)
    m = f.transfer(a, b)
    assert m > 0 and abs(a.total + b.total - 8.0) < 1e-9


def test_one_way_blocks_backflow():
    a = f.Volume(8.0); b = f.Volume(8.0); b.amounts[6] = 8.0
    assert f.transfer(a, b, one_way=True) == 0.0
