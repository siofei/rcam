"""Capture lifecycle/order/negative tests; synthetic unit data, not native PASS."""
import copy,unittest
from pmix_capture_lifecycle import Lifecycle
from run_pmix_native import display_policy_operation

class Tests(unittest.TestCase):
    def setUp(self):
        self.ready={'schema_version':2,'event':'ready','producer_pid':3,'app_pid':1,'window_id':2,'run_id':'unit','source_kind':'owned-window-sck','clock':'DispatchTime.uptimeNanoseconds','producer_started_ns':10,'at_ns':20,'accepted_samples':1,'width':64,'height':48,'frame_status':'complete','writer_status':'writing','sample_append_succeeded':True,'sample_pts_value':1,'sample_pts_timescale':60}
        self.done={'app_pid':1,'run_id':'unit','frame_id':50,'at_ns':1000}
        self.final=dict(self.ready,event='finished',at_ns=40,stop_requested_ns=30,writer_status='completed',stream_stopped=True,sample_queue_drained=True,input_marked_finished=True,finish_writing_callback_received=True,output_bytes=1000)
        self.movie={'samples':1,'duration_seconds':1/60,'width':64,'height':48}
    def life(self):return Lifecycle(1,2,'unit',3)
    def stopping(self):
        life=self.life();life.first_frame(self.ready);life.protocol_end(self.done);life.stop_requested();return life
    def test_short_source_normal_join_release(self):
        life=self.stopping();life.producer_joined(self.final,0,self.movie);life.release_app();life.app_exited(0);self.assertEqual(life.state,'COMPLETE')
    def test_no_frame_cannot_start_protocol_or_stop(self):
        for action in [lambda x:x.protocol_end(self.done),lambda x:x.stop_requested(),lambda x:x.release_app()]:
            with self.assertRaises(ValueError):action(self.life())
    def test_live_pid_does_not_make_ready(self):
        row=copy.deepcopy(self.ready);row['sample_append_succeeded']=False
        with self.assertRaises(ValueError):self.life().first_frame(row)
    def test_blank_or_idle_is_not_first_frame(self):
        for status in ['idle','blank','started','suspended']:
            row=dict(self.ready,frame_status=status)
            with self.assertRaises(ValueError):self.life().first_frame(row)
    def test_synthetic_writer_is_not_owned_window_evidence(self):
        row=dict(self.ready,source_kind='synthetic-self-test')
        with self.assertRaises(ValueError):self.life().first_frame(row)
    def test_short_app_exit_before_finalization_fails(self):
        for life in [self.life(),self.stopping()]:
            with self.assertRaises(ValueError):life.app_exited(0)
    def test_stopped_callback_is_not_writer_finish(self):
        for key in ['stream_stopped','sample_queue_drained','input_marked_finished','finish_writing_callback_received']:
            row=dict(self.final);row[key]=False
            with self.assertRaises(ValueError):self.stopping().producer_joined(row,0,self.movie)
    def test_failure_signal_or_nonzero_never_release(self):
        for code in [-15,-9,1,2,False]:
            with self.assertRaises(ValueError):self.stopping().producer_joined(self.final,code,self.movie)
    def test_no_valid_movie_never_release(self):
        for movie in [dict(self.movie,samples=0),dict(self.movie,duration_seconds=0),dict(self.movie,width=65)]:
            with self.assertRaises(ValueError):self.stopping().producer_joined(self.final,0,movie)
    def test_writer_failure_never_release(self):
        for status in ['writing','failed','cancelled']:
            with self.assertRaises(ValueError):self.stopping().producer_joined(dict(self.final,writer_status=status),0,self.movie)
    def test_reused_wrong_owner_or_pts(self):
        for key,value in [('app_pid',9),('window_id',9),('producer_pid',9),('run_id','other'),('sample_pts_timescale',0)]:
            row=dict(self.ready);row[key]=value
            with self.assertRaises(ValueError):self.life().first_frame(row)
    def test_finalization_before_ready_or_stop_fails(self):
        with self.assertRaises(ValueError):self.life().producer_joined(self.final,0,self.movie)
        life=self.life();life.first_frame(self.ready)
        with self.assertRaises(ValueError):life.producer_joined(self.final,0,self.movie)
    def test_duplicate_ready_fails(self):
        life=self.life();life.first_frame(self.ready)
        with self.assertRaises(ValueError):life.first_frame(self.ready)
    def test_preserve_default_no_settings_change(self):
        self.assertEqual(display_policy_operation('preserve',False),'probe')
    def test_frozen_60hz_requires_explicit_authorization(self):
        with self.assertRaises(ValueError):display_policy_operation('frozen-60hz',False)
        self.assertEqual(display_policy_operation('frozen-60hz',True),'set60')

if __name__=='__main__':unittest.main()
