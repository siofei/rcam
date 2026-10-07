"""Invented PMIX input receipts; parser tests, never native/GUI evidence."""
import copy
import json
import unittest
from pathlib import Path

from verify_pmix_native import verify_input_modifiers

ROOT=Path(__file__).resolve().parents[1]


def modifiers(alt=False,command=False,shift=False):
    return dict(alt=alt,ctrl=False,shift=shift,command=command,mac_cmd=command)


def frame(number,phase,incoming,mode,pressed=False,released=False,key=None,shift=False):
    mods=modifiers(alt=mode in ('move','escape','new-project') and incoming in (4,5,6,7),
                   command=key=='Z',shift=shift)
    buttons=[dict(button='Primary',pressed=value,position=[100.,100.],modifiers=copy.deepcopy(mods))
             for value,exists in ((True,pressed),(False,released)) if exists]
    keys=[dict(key=key,physical_key=key,pressed=value,repeat=False,modifiers=copy.deepcopy(mods))
          for value in (True,False)] if key else []
    injected=dict(input_policy_version=1,input_phase=incoming,modifiers=copy.deepcopy(mods),
                  buttons=buttons,keys=keys,pressed=pressed,released=released,pointer=[100.,100.])
    return dict(id=number,phase=phase,injected=injected,processed_modifiers=copy.deepcopy(mods),
                processed_buttons=copy.deepcopy(buttons),processed_keys=copy.deepcopy(keys))


class InputModifiers(unittest.TestCase):
    def setUp(self):
        self.policy=json.loads((ROOT/'fixtures/synthetic/s5m2c/native-inputs.json').read_text())
    def fixture(self,mode='move'):
        if mode in ('move','escape','new-project'):
            frames=[frame(1,3,3,mode),frame(2,5,4,mode,pressed=True),frame(3,5,5,mode),
                    frame(4,6,6,mode),frame(5,7,6,mode,released=True,key='Escape' if mode=='escape' else None),
                    frame(6,7,7,mode),frame(7,7,7,mode)]
            events=[dict(label='press',frame_id=2),dict(label='release',frame_id=5)]
            if mode=='move':
                frames+=[frame(8,9,8,mode,key='Z'),frame(9,9,9,mode),frame(10,11,10,mode,key='Z',shift=True),frame(11,12,12,mode),frame(12,14,14,mode)]
                events+=[dict(label='undo-input',frame_id=8),dict(label='redo-input',frame_id=10)]
            else:frames+=[frame(8,12,12,mode),frame(9,14,14,mode)]
        else:
            active={'nav':20,'points':31,'workflow':70,'workflow-reopen':12,'workflow-cross-layer':70}[mode]
            frames=[frame(1,1,1,mode),frame(2,active,active,mode),frame(3,14,14,mode)];events=[]
        return dict(request=dict(mode=mode),events=events),frames
    def check(self,mode='move'):
        report,frames=self.fixture(mode);verify_input_modifiers(report,frames,self.policy);return report,frames
    def reject(self,mutate,mode='move'):
        report,frames=self.check(mode);mutate(report,frames)
        with self.assertRaises((ValueError,KeyError,TypeError)):verify_input_modifiers(report,frames,self.policy)
    def test_all_modes_reset_or_bypass_and_release_cancel_shortcuts(self):
        for mode in ('move','escape','new-project','nav','points','workflow','workflow-reopen','workflow-cross-layer'):
            with self.subTest(mode=mode):self.check(mode)
        report,frames=self.fixture('points')
        frames[1]=frame(2,33,32,'points',pressed=True,released=True)
        verify_input_modifiers(report,frames,self.policy)
        frames=[frame(1,36,35,'points',pressed=True),frame(2,37,36,'points'),frame(3,38,37,'points',released=True)]
        verify_input_modifiers(report,frames,self.policy)
    def test_missing_or_inactive_raw_and_processed_bypass_are_rejected(self):
        for index in (1,2,3,4,5):
            for field in ('modifiers','processed_modifiers'):
                self.reject(lambda _,frames,index=index,field=field:(frames[index]['injected']['modifiers'] if field=='modifiers'
                            else frames[index]['processed_modifiers']).update(alt=False))
        self.reject(lambda _,frames:frames[3]['injected'].pop('modifiers'))
        self.reject(lambda _,frames:frames[3].pop('processed_modifiers'))
    def test_modifier_shapes_boolean_types_and_extra_commands_are_rejected(self):
        for key,value in (('alt',1),('ctrl',0),('ctrl',True),('shift',True),('command',True),('mac_cmd',True)):
            self.reject(lambda _,frames,key=key,value=value:frames[3]['injected']['modifiers'].update({key:value}))
        self.reject(lambda _,frames:frames[3]['injected']['modifiers'].pop('mac_cmd'))
        self.reject(lambda _,frames:frames[3]['injected']['modifiers'].update(extra=False))
    def test_button_modifier_delivery_and_exact_count_are_required(self):
        for index in (1,4):
            self.reject(lambda _,frames,index=index:frames[index]['injected']['buttons'][0]['modifiers'].update(alt=False))
            self.reject(lambda _,frames,index=index:frames[index]['processed_buttons'][0]['modifiers'].update(alt=False))
            self.reject(lambda _,frames,index=index:frames[index]['injected'].update(buttons=[]))
            self.reject(lambda _,frames,index=index:frames[index]['injected']['buttons'].append(copy.deepcopy(frames[index]['injected']['buttons'][0])))
        self.reject(lambda _,frames:frames[1]['processed_buttons'][0].update(pressed=1))
    def test_escape_event_preserves_alt_and_cannot_be_relabelled(self):
        for key,value in (('key','Z'),('physical_key','Z'),('pressed',0),('repeat',0)):
            self.reject(lambda _,frames,key=key,value=value:frames[4]['injected']['keys'][0].update({key:value}),mode='escape')
        self.reject(lambda _,frames:frames[4]['injected']['keys'][0]['modifiers'].update(alt=False),mode='escape')
        self.reject(lambda _,frames:frames[4]['processed_keys'][0]['modifiers'].update(alt=False),mode='escape')
        self.reject(lambda _,frames:frames[4]['injected'].update(keys=[]),mode='escape')
    def test_bypass_leakage_into_navigation_workflow_and_shortcuts_is_rejected(self):
        for mode in ('nav','points','workflow','workflow-reopen','workflow-cross-layer'):
            self.reject(lambda _,frames:frames[1]['injected']['modifiers'].update(alt=True),mode=mode)
        for index in (7,9):
            self.reject(lambda _,frames,index=index:frames[index]['injected']['modifiers'].update(alt=True))
        self.reject(lambda _,frames:frames[9]['injected']['modifiers'].update(shift=False))
    def test_missing_legacy_or_false_declaration_and_phase_are_rejected(self):
        self.reject(lambda _,frames:frames[3]['injected'].pop('input_policy_version'))
        self.reject(lambda _,frames:frames[3]['injected'].update(input_policy_version=True))
        self.reject(lambda _,frames:frames[3]['injected'].update(input_phase=3))
        for key,value in (('version',False),('bypass','global-snap-off')):
            report,frames=self.check();policy=copy.deepcopy(self.policy);policy['drag_input_policy'][key]=value
            with self.assertRaises(ValueError):verify_input_modifiers(report,frames,policy)
    def test_key_record_boolean_alias_and_extra_event_are_rejected(self):
        self.reject(lambda _,frames:frames[7]['processed_keys'][0].update(pressed=1))
        self.reject(lambda _,frames:frames[7]['injected']['keys'].append(copy.deepcopy(frames[7]['injected']['keys'][0])))
        self.reject(lambda _,frames:frames[7]['injected']['keys'][0]['modifiers'].update(ctrl=True))
    def test_missing_foreign_anchors_and_relabelled_shortcut_phases_are_rejected(self):
        self.reject(lambda report,_:report['events'].pop(0))
        self.reject(lambda report,_:report['events'][0].update(frame_id=True))
        self.reject(lambda report,_:report['events'].append(dict(label='undo-input',frame_id=2)),mode='workflow')
        self.reject(lambda _,frames:frames[7]['injected'].update(input_phase=12))
        self.reject(lambda _,frames:frames[9]['injected'].update(input_phase=12))
        self.reject(lambda _,frames:frames[2]['injected'].update(pressed=True))
        self.reject(lambda _,frames:frames[1]['injected'].update(pressed=True),mode='nav')
    def test_impossible_phase_pairs_and_foreign_mode_phases_are_rejected(self):
        self.reject(lambda _,frames:frames[0]['injected'].update(input_phase=8))
        self.reject(lambda _,frames:frames[1]['injected'].update(input_phase=8),mode='nav')
        self.reject(lambda _,frames:frames[0]['injected'].update(input_phase=True))
        self.reject(lambda _,frames:frames[1].update(phase=20),mode='points')
        self.reject(lambda _,frames:frames[1].update(phase=20),mode='workflow')
        self.reject(lambda _,frames:frames[1].update(phase=70),mode='workflow-reopen')
        self.reject(lambda _,frames:frames[1]['injected'].update(input_phase=75),mode='workflow-cross-layer')
    def test_unanchored_press_release_and_history_transitions_are_rejected(self):
        self.reject(lambda _,frames:frames[2]['injected'].update(input_phase=4))
        self.reject(lambda _,frames:frames[5]['injected'].update(input_phase=6))
        self.reject(lambda _,frames:frames[8]['injected'].update(input_phase=8))
        self.reject(lambda _,frames:frames[10]['injected'].update(input_phase=10))
        self.reject(lambda _,frames:frames[1].update(phase=33,injected=dict(frames[1]['injected'],input_phase=32)),mode='points')


if __name__=='__main__':unittest.main()
