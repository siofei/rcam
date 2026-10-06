"""Portable owned-window capture checks. Validate framing, not codec decoding.

No external tools, online services, or desktop access. Native producer receipts
must come from run_pmix_native; parser fixtures do not earn native acceptance.
"""
import math
import struct
import zlib
from pathlib import Path
from verify_s5m2_evidence import load, require, safe
from verify_pmix_workflow import parse_json
from run_pmix_native import WINDOW_SWIFT
from pmix_owned_command import verify_command

MAX_MEDIA_BYTES = 1024 * 1024 * 1024

def integer(value, minimum, label):
    require(type(value) is int and minimum <= value < 2**63, label)
    return value


def png_info(path):
    require(0 < path.stat().st_size <= 64*1024*1024, 'PNG size budget')
    data = path.read_bytes()
    require(data[:8] == b'\x89PNG\r\n\x1a\n', 'actual PNG signature')
    at = 8; chunks = []; compressed = bytearray(); width = height = depth = color = None
    while at < len(data):
        require(at+12 <= len(data), 'truncated PNG chunk')
        length = struct.unpack_from('>I', data, at)[0]; kind = data[at+4:at+8]
        end = at+12+length
        require(end <= len(data), 'PNG chunk bounds')
        payload = data[at+8:at+8+length]
        require(zlib.crc32(kind+payload)&0xffffffff == struct.unpack_from('>I', data, end-4)[0], 'PNG CRC')
        if not chunks:
            require(kind == b'IHDR' and length == 13, 'PNG IHDR')
            width,height,depth,color,compression,filtering,interlace = struct.unpack('>IIBBBBB',payload)
            require(0 < width <= 16384 and 0 < height <= 16384 and width*height <= 32*1024*1024, 'PNG dimensions budget')
            require(depth in (8,16) and color in (0,2,4,6) and compression == filtering == interlace == 0, 'PNG native screenshot format')
        elif kind == b'IHDR': raise ValueError('duplicate PNG IHDR')
        if kind == b'IDAT':
            require(b'IDAT' not in chunks or chunks[-1] == b'IDAT', 'noncontiguous PNG IDAT')
            compressed.extend(payload)
        if kind == b'IEND':
            require(length == 0 and end == len(data) and compressed, 'PNG IEND/data coverage')
        chunks.append(kind); at = end
    require(chunks[-1:] == [b'IEND'], 'missing PNG IEND')
    channels = {0:1,2:3,4:2,6:4}[color]; row_bytes = width*channels*(depth//8)+1
    expected = row_bytes*height
    require(expected <= 256*1024*1024, 'PNG inflated budget')
    inflater = zlib.decompressobj()
    pixels = inflater.decompress(bytes(compressed), expected+1)
    require(inflater.eof and not inflater.unused_data and not inflater.unconsumed_tail and len(pixels) == expected, 'PNG decompressed row coverage')
    require(all(pixels[i*row_bytes] <= 4 for i in range(height)), 'PNG scanline filter')
    return {'width':width,'height':height}


def mov_info(path):
    size = path.stat().st_size
    require(128 < size <= MAX_MEDIA_BYTES, 'MOV size budget')
    with path.open('rb') as stream:
        def boxes(start,end,entry_padding=False):
            result=[]; at=start
            while at < end:
                if entry_padding and end-at==4:
                    stream.seek(at)
                    require(stream.read(4)==b'\0'*4,'MOV sample entry trailing padding')
                    break
                require(at+8 <= end and len(result)<100000, 'MOV box header/budget')
                stream.seek(at); raw=stream.read(8); length,kind=struct.unpack('>I4s',raw); header=8
                if length==1:
                    require(at+16 <= end, 'MOV extended box'); length=struct.unpack('>Q',stream.read(8))[0]; header=16
                elif length==0: length=end-at
                require(length>=header and at+length<=end, 'MOV box bounds/truncation')
                result.append((kind,at+header,at+length)); at+=length
            return result
        def one(rows,kind):
            matches=[row for row in rows if row[0]==kind]
            require(len(matches)==1, 'MOV required unique box '+repr(kind))
            return matches[0]
        def children(row):return boxes(row[1],row[2])
        def payload(row):
            require(row[2]-row[1]<=16*1024*1024, 'MOV metadata budget')
            stream.seek(row[1]);return stream.read(row[2]-row[1])
        def table(row,columns):
            raw=payload(row); require(len(raw)>=8 and raw[:4]==b'\0'*4, 'MOV table version')
            count=struct.unpack_from('>I',raw,4)[0]
            require(count<=1000000 and len(raw)==8+count*4*columns, 'MOV table count/length')
            return [struct.unpack_from('>'+'I'*columns,raw,8+i*4*columns) for i in range(count)]
        top=boxes(0,size); moov=one(top,b'moov'); mdats=[(row[1],row[2]) for row in top if row[0]==b'mdat']
        require(mdats and all(end>start for start,end in mdats), 'MOV media payload')
        tracks=[]
        for track in [x for x in children(moov) if x[0]==b'trak']:
            mdia=children(one(children(track),b'mdia')); handler=payload(one(mdia,b'hdlr'))
            require(len(handler)>=12, 'MOV handler')
            if handler[8:12]!=b'vide':continue
            header=payload(one(mdia,b'mdhd'))
            require(len(header)>=24 and header[0] in (0,1), 'MOV media header')
            if header[0]==0:timescale,duration=struct.unpack_from('>II',header,12)
            else:
                require(len(header)>=36, 'MOV extended media header');timescale=struct.unpack_from('>I',header,20)[0];duration=struct.unpack_from('>Q',header,24)[0]
            require(timescale>0 and duration>0, 'MOV video duration')
            stbl=children(one(children(one(mdia,b'minf')),b'stbl'))
            descriptions=payload(one(stbl,b'stsd'))
            require(len(descriptions)>=8 and descriptions[:4]==b'\0'*4 and struct.unpack_from('>I',descriptions,4)[0]==1, 'MOV video description count/version')
            require(len(descriptions)>=44, 'MOV video sample entry')
            length,codec=struct.unpack_from('>I4s',descriptions,8)
            require(length>=86 and len(descriptions)==length+8 and codec==b'avc1', 'MOV supported native AVC video entry')
            configuration=payload(one(boxes(one(stbl,b'stsd')[1]+8+86,one(stbl,b'stsd')[2],entry_padding=True),b'avcC'))
            require(len(configuration)>=7 and configuration[0]==1, 'MOV AVC configuration')
            nal_length_bytes=(configuration[4]&3)+1
            require(nal_length_bytes in (1,2,4) and configuration[5]&31, 'MOV AVC NAL framing/SPS')
            config_at=6
            for group in (7,8):
                n=configuration[5]&31 if group==7 else configuration[config_at]
                if group==8:config_at+=1
                require(n>0, 'MOV AVC parameter set count')
                for _ in range(n):
                    require(config_at+2<=len(configuration),'MOV AVC configuration truncation')
                    nbytes=struct.unpack_from('>H',configuration,config_at)[0];config_at+=2
                    require(nbytes>0 and config_at+nbytes<=len(configuration) and configuration[config_at]&31==group,'MOV AVC parameter set bounds/type')
                    config_at+=nbytes
                if group==7:require(config_at<len(configuration),'MOV AVC missing PPS')
            width,height=struct.unpack_from('>HH',descriptions,40)
            require(0<width<=16384 and 0<height<=16384, 'MOV video dimensions')
            timing=table(one(stbl,b'stts'),2)
            require(timing and all(count>0 and delta>0 for count,delta in timing), 'MOV nonempty timing')
            count=sum(row[0] for row in timing); ticks=sum(n*d for n,d in timing)
            require(0<count<=1000000 and abs(ticks-duration)<=max(d for n,d in timing), 'MOV sample duration coverage')
            sizes=payload(one(stbl,b'stsz'));require(len(sizes)>=12 and sizes[:4]==b'\0'*4, 'MOV sample size table')
            fixed,n=struct.unpack_from('>II',sizes,4);require(n==count, 'MOV sample count binding')
            require(len(sizes)==(12 if fixed else 12+n*4), 'MOV sample size table length')
            sample_sizes=[fixed]*n if fixed else list(struct.unpack_from('>'+'I'*n,sizes,12))
            require(all(v>0 for v in sample_sizes), 'MOV zero sample bytes')
            offset_boxes=[x for x in stbl if x[0] in (b'stco',b'co64')];require(len(offset_boxes)==1,'MOV unique chunk offsets')
            offsets_raw=payload(offset_boxes[0]);require(len(offsets_raw)>=8 and offsets_raw[:4]==b'\0'*4, 'MOV offset header')
            n_offsets=struct.unpack_from('>I',offsets_raw,4)[0];unit=4 if offset_boxes[0][0]==b'stco' else 8
            require(0<n_offsets<=count and len(offsets_raw)==8+n_offsets*unit,'MOV offset table count/length')
            offsets=struct.unpack_from('>'+('I' if unit==4 else 'Q')*n_offsets,offsets_raw,8)
            mapping=table(one(stbl,b'stsc'),3)
            require(mapping and mapping[0][0]==1 and [x[0] for x in mapping]==sorted({x[0] for x in mapping}) and all(1<=first<=n_offsets and samples>0 and description==1 for first,samples,description in mapping),'MOV chunk sample map')
            index=0; map_index=0; extents=[]
            for chunk,offset in enumerate(offsets,1):
                while map_index+1<len(mapping) and mapping[map_index+1][0]<=chunk:map_index+=1
                samples=mapping[map_index][1];require(index+samples<=count,'MOV excess chunk samples')
                end=offset+sum(sample_sizes[index:index+samples]);index+=samples
                require(any(start<=offset<end<=limit for start,limit in mdats), 'MOV video sample outside mdat')
                sample_at=offset
                for sample_size in sample_sizes[index-samples:index]:
                    sample_end=sample_at+sample_size;nal_count=0
                    while sample_at<sample_end:
                        require(sample_at+nal_length_bytes<sample_end, 'MOV truncated AVC NAL prefix')
                        stream.seek(sample_at);nal_size=int.from_bytes(stream.read(nal_length_bytes),'big')
                        sample_at+=nal_length_bytes
                        require(nal_size>0 and sample_at+nal_size<=sample_end, 'MOV AVC NAL bounds')
                        header_byte=stream.read(1)[0]
                        require(header_byte&128==0 and 1<=header_byte&31<=23, 'MOV AVC NAL header')
                        sample_at+=nal_size;nal_count+=1
                        require(nal_count<=100000, 'MOV AVC NAL count budget')
                    require(nal_count>0,'MOV empty AVC sample')
                extents.append((offset,end))
            require(index==count,'MOV missing chunk samples')
            extents.sort();require(all(a[1]<=b[0] for a,b in zip(extents,extents[1:])), 'MOV overlapping samples')
            tracks.append({'width':width,'height':height,'duration_seconds':ticks/timescale,'samples':count,'codec':codec.decode('ascii'),'sample_bytes':sum(sample_sizes)})
        require(len(tracks)==1,'MOV exactly one video track')
        return tracks[0]


def capture_command(command,window_id,basename):
    require(type(command) is list and len(command)==6 and command[:4]==['/usr/sbin/screencapture','-x','-o','-l'] and command[-2]==str(window_id),'owned-window image command/window binding')
    require(Path(command[-1]).is_absolute() and Path(command[-1]).name==basename,'image output path')


def native_platform(rows, video):
    stages=['bootstrap-attempt','bootstrap','permission-check','filter-before','filter-after',
            'stream-created','stream-started','stream-stop-completed','writer-terminal']
    require(type(rows) is list and [row['stage'] for row in rows]==stages,
            'native platform exact startup/terminal trace; initialization/synthetic/error cannot substitute')
    require(all(type(row['producer_pid']) is int and row['producer_pid']==video['pid']
                and type(row['thread_main']) is bool and type(row['at_ns']) is int and row['at_ns']>0 for row in rows)
            and all(a['at_ns']<b['at_ns'] for a,b in zip(rows,rows[1:])), 'native platform PID/clock')
    require(all(row['thread_main'] is True for row in rows[:7]), 'native physical mainthread startup')
    require(type(rows[0]['after_policy_raw']) is int and rows[0]['after_policy_raw']==2
            and rows[0]['active'] is False and type(rows[0]['own_windows']) is int and rows[0]['own_windows']==0
            and rows[1]['activation_policy']=='prohibited' and rows[1]['active'] is False
            and type(rows[1]['own_windows']) is int and rows[1]['own_windows']==0
            and rows[2]['existing_access'] is True and rows[2]['request_called'] is False, 'native non-GUI bootstrap/access')
    require(rows[3]['query_kind']==rows[4]['query_kind']=='excludingDesktopWindows-owned'
            and rows[3]['eligible_real_window'] is True and rows[4]['constructor']=='desktopIndependentWindow'
            and type(rows[5]['app_pid']) is int and rows[5]['app_pid']==video['app_pid']
            and type(rows[5]['window_id']) is int and rows[5]['window_id']==video['window_id']
            and rows[-1]['terminal']=='completed', 'native genuine constructor/stream/terminal binding')
    first=video['first_frame'];last=video['finished_event']
    require(rows[1]['at_ns']<first['producer_started_ns']<=rows[2]['at_ns']
            and rows[5]['at_ns']<=first['at_ns']<=rows[7]['at_ns']
            and last['stop_requested_ns']<=rows[7]['at_ns']<=last['at_ns']<=rows[8]['at_ns'],
            'native startup/frame/stop/writer clock binding')


def capture_receipts(directory,r,expected_producer_sha256):
    from pmix_capture_lifecycle import Lifecycle
    from pmix_capture_swift import SOURCE
    import hashlib
    owned=load(safe(directory,'owned-process.json'));usage=load(safe(directory,'owned-resource-usage.json'))
    query=load(safe(directory,'window-query.json'));windows=load(safe(directory,'window.json'))
    image=load(safe(directory,'image-command.json'));video=load(safe(directory,'video-command.json'))
    ready=load(safe(directory,'capture-ready.json'));done=load(safe(directory,'protocol-done.json'));complete=load(safe(directory,'capture-complete.json'))
    pid=integer(r['pid'],1,'owned app PID')
    require(query['pid']==owned['pid']==usage['pid']==pid and type(query['exit_code']) is int and query['exit_code']==0,'window-query owned PID/exit')
    require(type(query['command']) is list and len(query['command'])==3 and query['command'][0]=='/usr/bin/swift' and Path(query['command'][1]).is_absolute() and Path(query['command'][1]).name=='window.swift' and query['command'][2]==str(pid),'window query command')
    require(safe(directory,'window.swift').read_text()==WINDOW_SWIFT and safe(directory,'capture.swift').read_text()==SOURCE,'reviewed window/capture producer source')
    require(query['stderr']=='' and parse_json(query['stdout'].encode())==windows==query['windows'] and windows,'window actual query output')
    selected=[w for w in windows if w['window_id']==video['window_id']];require(len(selected)==1,'selected window missing/duplicate')
    window=selected[0];wid=integer(window['window_id'],1,'window ID')
    require(type(window['owner_pid']) is int and window['owner_pid']==pid and type(window['layer']) is int and window['layer']==0,'window actual owner/layer binding')
    require(image['window_id']==video['window_id']==wid and image['app_pid']==video['app_pid']==pid,'capture app/window producer binding')
    for producer in (query,image,video):
        start=integer(producer['started_monotonic_ns'],1,'producer start clock');end=integer(producer['finished_monotonic_ns'],start,'producer end clock');require(end>start,'producer elapsed clock')
    require(query['finished_monotonic_ns']<=image['started_monotonic_ns']<=image['finished_monotonic_ns']<=video['started_monotonic_ns'],'capture producer ordering')
    capture_command(image['command'],wid,'native-window.png')
    require(type(image['exit_code']) is int and image['exit_code']==0 and image['stderr']=='','image capture exit/stderr')
    for label,receipt in (('window-query',query),('image',image)):
        bounded=verify_command(directory,label,receipt['command'],receipt['exit_code'],receipt['stdout'],receipt['stderr'])
        require(receipt['started_monotonic_ns']<=bounded['started_monotonic_ns']<bounded['finished_monotonic_ns']<=receipt['finished_monotonic_ns'], 'bounded helper within query/image clock')
    producer_path=safe(directory,'capture-producer');require(hashlib.sha256(producer_path.read_bytes()).hexdigest()==video['producer_sha256']==expected_producer_sha256,'external gate-bound capture producer binary')
    command=video['command'];require(type(command) is list and len(command)==9 and Path(command[0]).is_absolute() and Path(command[0]).name=='capture-producer' and command[1:4]==['--owned-window',str(pid),str(wid)] and Path(command[4]).name=='native-window.mov' and command[5]==r['request']['run_id'] and command[6]=='H264-MOV','owned-window capture command/window binding')
    require(Path(command[0]).parent==Path(command[4]).parent==Path(query['command'][1]).parent==Path(image['command'][-1]).parent,'capture query/output directory binding')
    integer(video['pid'],1,'video child PID');require(video['pid']!=pid and type(video['exit_code']) is int and video['exit_code']==0,'video capture exit')
    require(video['stderr']==safe(directory,'capture-stderr.log').read_text()=='' and video['stdout']==safe(directory,'capture-stdout.log').read_text(),'capture raw stdout/stderr')
    rows=[parse_json(line.encode()) for line in video['stdout'].splitlines()]
    require(rows==load(safe(directory,'capture-events.json'))==[video['first_frame'],video['finished_event']],'actual first/final producer event coverage')
    require(video['joined_before_app_release'] is True and video['finalization']=='SCStream.stopCapture + drain + AVAssetWriter.finishWriting + process.join','video capture finalization/join')
    launch=load(safe(directory,'owned-producer.json'));cleanup=load(safe(directory,'producer-cleanup.json'))
    owned_cleanup=load(safe(directory,'owned-cleanup.json'))
    require(launch==dict({key:video[key] for key in ('pid','app_pid','window_id','command','producer_sha256','started_monotonic_ns')},
                        pgid=video['pid'],private_session=True), 'immediate owned producer launch binding')
    require(type(cleanup['pid']) is int and cleanup['pid']==video['pid'] and type(cleanup['exit_code']) is int
            and cleanup['exit_code']==0 and cleanup['signal'] is None and cleanup['joined'] is True
            and type(cleanup['pgid']) is int and cleanup['pgid']==video['pid'] and cleanup['private_session'] is True
            and cleanup['owned_group_released'] is True and cleanup['signals_sent']==[]
            and cleanup['reader_joined'] is True and cleanup['control_state']=='COMPLETE'
            and cleanup['scope']=='owned producer actual cleanup; never native success', 'owned producer actual join/cleanup')
    require(type(owned_cleanup['app_pid']) is int and owned_cleanup['app_pid']==pid
            and type(owned_cleanup['producer_pid']) is int and owned_cleanup['producer_pid']==video['pid']
            and type(owned_cleanup['app_exit_code']) is int and owned_cleanup['app_exit_code']==0
            and type(owned_cleanup['producer_exit_code']) is int and owned_cleanup['producer_exit_code']==0
            and owned_cleanup['producer_reader_joined'] is True
            and owned_cleanup['scope']=='cleanup status; resource accounting only from owned wait4', 'owned app/producer cleanup binding')
    native_platform([parse_json(line.encode()) for line in safe(directory,'capture-platform.jsonl').read_text().splitlines()],video)
    app_start=integer(usage['started_monotonic_ns'],1,'owned app start clock');app_exit=integer(usage['finished_monotonic_ns'],1,'owned wait4 completion clock')
    require(integer(cleanup['finished_monotonic_ns'],1,'producer cleanup clock')>=video['finished_monotonic_ns']
            and integer(owned_cleanup['finished_monotonic_ns'],1,'owned cleanup clock')>=max(app_exit,cleanup['finished_monotonic_ns']), 'actual cleanup completion clock')
    require(app_start<=query['started_monotonic_ns'],'owned app/window query clock')
    release=integer(complete['release_monotonic_ns'],1,'app release clock')
    require(video['started_monotonic_ns']<video['ready_monotonic_ns']==ready['ready_monotonic_ns']<video['stop_requested_monotonic_ns']<=video['finished_monotonic_ns']==complete['producer_finished_monotonic_ns']<=release<app_exit,'first frame/stop/join/app release clock ordering')
    require(ready['app_pid']==complete['app_pid']==pid and ready['run_id']==complete['run_id']==r['request']['run_id'] and ready['video_pid']==complete['video_pid']==video['pid'] and ready['video_started_monotonic_ns']==video['started_monotonic_ns'] and ready['first_frame']==video['first_frame'] and complete['success'] is True,'capture ready/completion owned producers')
    require(complete['movie_sha256']==hashlib.sha256(safe(directory,'native-window.mov').read_bytes()).hexdigest(),'movie validated before app release')
    def event(label,data):
        matches=[e for e in r['events'] if e['label']==label];require(len(matches)==1 and matches[0]['data']==data,'actual app '+label+' receipt');return matches[0]
    start_event=event('capture-ready',ready);end_event=event('protocol-end',done);end_capture=event('capture-complete',complete)
    marker=load(safe(directory,'window-ready.json'));window_event=event('window-ready',marker)
    require(window_event['at_ns']<start_event['at_ns']<end_event['at_ns']<end_capture['at_ns'],'app window/capture/protocol/release event ordering')
    require(done['at_ns']==end_event['at_ns'] or abs(done['at_ns']-end_event['at_ns'])<1000000,'protocol end independent clock binding')
    require(done['frame_id']==end_event['frame_id'],'protocol end frame binding')
    require(all(start_event['at_ns']<=e['at_ns']<=end_event['at_ns'] for e in r['events'] if e['label'] not in ('window-ready','capture-ready','protocol-end','capture-complete')),'protocol event outside capture-ready/end')
    png=png_info(safe(directory,'native-window.png'));movie=mov_info(safe(directory,'native-window.mov'))
    life=Lifecycle(pid,wid,r['request']['run_id'],video['pid']);life.first_frame(video['first_frame']);life.protocol_end(done);life.stop_requested();life.producer_joined(video['finished_event'],video['exit_code'],movie);life.release_app();life.app_exited(usage['exit_code'])
    require(movie['samples']==video['finished_event']['accepted_samples'],'every accepted capture frame encoded')
    require(png['width']==movie['width'] and png['height']==movie['height'] and command[7:]==[str(movie['width']),str(movie['height'])],'image/movie/command window dimensions')
    bounds=window['bounds'];require(all(type(bounds[k]) in (int,float) and math.isfinite(bounds[k]) for k in ('X','Y','Width','Height')),'window bounds')
    scale=load(safe(directory,'display-active-probe.json'))['after']['backing_scale']
    require(abs(movie['width']-bounds['Width']*scale)<=2 and abs(movie['height']-bounds['Height']*scale)<=2,'capture/window physical dimensions')
    protocol_span=(end_event['at_ns']-start_event['at_ns'])/1e9
    require(protocol_span>0 and movie['duration_seconds']+1>=protocol_span,'MOV complete protocol time coverage')
    require(movie['duration_seconds']<=(video['finished_monotonic_ns']-video['started_monotonic_ns'])/1e9+1,'MOV producer duration bound')
    return {'window_id':wid,'app_pid':pid,'png':png,'movie':movie,'capture_state':life.state,'codec_decode_verified':False,'protocol_span_seconds':protocol_span,'capture_tail_scope':'all raw app/ROI frames retained, movie drained before app close; tail is not a performance phase'}
