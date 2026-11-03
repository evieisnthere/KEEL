import os, sys, glob
import pytest
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..', 'src'))
from keel_io import losslessxml as X
from keel_io.vehicle import Vehicle

# Synthetic fixture written for KEEL (no game or workshop content). It mirrors observed structure.
VEHICLE = (b'<?xml version="1.0" encoding="UTF-8"?><vehicle data_version="3" bodies_id="2"><authors/>'
           b'<bodies><body unique_id="1"><components>'
           b'<c><o r="1,0,0,0,1,0,0,0,1" bc="FFFFFF" ac="FFFFFF" sc="6"><vp x="1"/></o></c>'
           b'<c d="02_wedge" t="2"><o r="0,0,1,1,0,0,0,1,0" bc="141414" ac="141414" sc="5"><vp x="2" y="-2" z="-4"/></o></c>'
           b'</components></body></bodies>'
           b'<logic_node_links><logic_node_link type="1"><voxel_pos_0 x="1"/><voxel_pos_1 z="3"/></logic_node_link></logic_node_links>'
           b'</vehicle>\n')
DEFINITION_QUIRK = (b'<definition name="X">\n\t<voxels>\n\t\t<voxel flags="1">\n'
                    b'\t\t\t<physics_shape_rotation 00="1" 01="0" 02="0" 10="0" 11="1" 12="0" 20="0" 21="0" 22="1"/>\n'
                    b'\t\t</voxel>\n\t</voxels>\n</definition>\n')


def test_roundtrip_identical():
    for src in (VEHICLE, DEFINITION_QUIRK):
        assert X.parse(src).to_bytes() == src


def test_digit_attribute_names():
    d = X.parse(DEFINITION_QUIRK)
    rot = next(d.root.iter('physics_shape_rotation'))
    assert rot.get('00') == '1' and rot.get('22') == '1'


def test_vehicle_model_reads():
    v = Vehicle(X.parse(VEHICLE))
    cs = list(v.components())
    assert [c.definition for c in cs] == ['01_block', '02_wedge']
    assert cs[0].voxel == (1, 0, 0) and cs[1].voxel == (2, -2, -4)
    assert cs[1].rotation == (0, 0, 1, 1, 0, 0, 0, 1, 0) and cs[1].variant == '2'
    assert v.logic_links()[0].endpoints == ((1, 0, 0), (0, 0, 3))


def test_local_edit_only_changes_that_element():
    v = Vehicle(X.parse(VEHICLE))
    c = list(v.components())[1]
    c.voxel = (2, 0, -4)
    out = v.doc.to_bytes()
    assert out == VEHICLE.replace(b'<vp x="2" y="-2" z="-4"/>', b'<vp x="2" z="-4"/>')


def test_malformed_is_reported():
    with pytest.raises(X.XmlError):
        X.parse(b'<vehicle><bodies></vehicle>')


CORPUS = os.environ.get('KEEL_CORPUS')


@pytest.mark.skipif(not CORPUS, reason='set KEEL_CORPUS to a folder of your own vehicle/definition XML files')
def test_corpus_roundtrip():
    files = glob.glob(os.path.join(CORPUS, '**', '*.xml'), recursive=True)
    assert files
    for p in files:
        b = open(p, 'rb').read()
        assert X.parse(b).to_bytes() == b, p
