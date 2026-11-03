import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..', 'sim-reference'))
import sw_rudder as R


def test_actuator_instant_when_powered_and_stuck_without():
    assert R.actuator(0.0, 1.0, 1.0) == 1.0
    assert R.actuator(0.0, 1.0, 0.0) == 0.0


def test_angle_range():
    assert abs(R.angle(1.0) - 0.785398) < 1e-9 and abs(R.angle(0.0)) < 1e-9


def test_force_opposes_normal_velocity_and_saturates():
    f = R.water_force((1, 0, 0), (2, 0, 0), 10000, 13, 1.0)
    assert f[0] == -2 * 400 * 10
    assert R.water_force((1, 0, 0), (2, 0, 0), 10000, 13, 1.0, keel=True)[0] == 40 * f[0]
