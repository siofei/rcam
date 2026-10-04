"""Strict capture coordination; pure state checks shared with unit tests."""
from verify_s5m2_evidence import require

class Lifecycle:
    def __init__(self,app_pid,window_id,run_id,producer_pid):
        self.app_pid=app_pid;self.window_id=window_id;self.run_id=run_id;self.producer_pid=producer_pid
        self.state='STARTING';self.ready=None;self.done=None;self.finished=None
    def identity(self,row):
        require(type(row) is dict and type(row.get('schema_version')) is int and row['schema_version']==2,'capture producer schema')
        require(row['app_pid']==self.app_pid and row['window_id']==self.window_id and row['producer_pid']==self.producer_pid and row['run_id']==self.run_id,'capture owned producer identity')
        require(row['source_kind']=='owned-window-sck' and row['clock']=='DispatchTime.uptimeNanoseconds','capture source/clock')
        for key in ('at_ns','producer_started_ns','accepted_samples','width','height'):
            require(type(row[key]) is int and row[key]>0,'capture producer positive '+key)
        require(row['producer_started_ns']<=row['at_ns'],'capture producer clock order')
    def first_frame(self,row):
        require(self.state=='STARTING','capture ready out of order');self.identity(row)
        require(row['event']=='ready' and row['frame_status']=='complete' and row['writer_status']=='writing' and row['sample_append_succeeded'] is True,'actual first frame/writer acceptance')
        require(type(row['sample_pts_value']) is int and type(row['sample_pts_timescale']) is int and row['sample_pts_timescale']>0,'first captured sample PTS')
        self.ready=row;self.state='READY'
    def protocol_end(self,row):
        require(self.state=='READY','protocol ended before real first frame')
        require(row['app_pid']==self.app_pid and row['run_id']==self.run_id and type(row['frame_id']) is int and row['frame_id']>0 and type(row['at_ns']) is int and row['at_ns']>0,'protocol end identity/frame/clock')
        self.done=row;self.state='PROTOCOL_DONE'
    def stop_requested(self):
        require(self.state=='PROTOCOL_DONE','stop before protocol done');self.state='STOP_REQUESTED'
    def producer_joined(self,row,exit_code,movie):
        require(self.state=='STOP_REQUESTED','capture finalized out of order');self.identity(row)
        require(type(exit_code) is int and exit_code==0 and row['event']=='finished','capture producer exit/final event')
        require(row['writer_status']=='completed' and all(row[key] is True for key in ('stream_stopped','sample_queue_drained','input_marked_finished','finish_writing_callback_received')),'capture stream drain/writer finalization')
        require(row['accepted_samples']>=self.ready['accepted_samples'] and row['producer_started_ns']==self.ready['producer_started_ns'] and row['width']==self.ready['width'] and row['height']==self.ready['height'],'capture first/final continuity')
        require(type(row['stop_requested_ns']) is int and self.ready['at_ns']<=row['stop_requested_ns']<=row['at_ns'],'capture stop/finish clock')
        require(type(row['output_bytes']) is int and row['output_bytes']>128 and movie['samples']>0 and movie['duration_seconds']>0,'finalized valid movie required')
        require(movie['width']==row['width'] and movie['height']==row['height'],'finished movie/frame dimensions')
        self.finished=row;self.state='FINALIZED'
    def release_app(self):
        require(self.state=='FINALIZED','app release before joined valid movie');self.state='RELEASED'
    def app_exited(self,exit_code):
        require(self.state=='RELEASED' and type(exit_code) is int and exit_code==0,'app early/nonzero exit before capture finalization');self.state='COMPLETE'
