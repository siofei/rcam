struct Uniforms { view:vec4<f32>, camera:vec4<f32>, counts:vec4<u32>, preview:vec4<f32>, grid:vec4<f32>, world:vec4<f32>, selection_bounds:vec4<f32> }
struct Object { tag:vec4<u32>, bounds:vec4<f32>, style:vec4<u32> }
struct Primitive { tag:vec4<u32>, a:vec4<f32>, b:vec4<f32>, bounds:vec4<f32> }
@group(0) @binding(0) var<uniform> u:Uniforms;
@group(0) @binding(1) var<storage,read> objects:array<Object>;
@group(0) @binding(2) var<storage,read> shapes:array<Primitive>;
@group(0) @binding(3) var<storage,read> points:array<vec2<f32>>;
@group(0) @binding(4) var<storage,read> selected:array<u32>;
@group(0) @binding(5) var<storage,read> bins:array<u32>;
fn cell(p:vec2<f32>)->u32 {
 if any(p<u.world.xy) || any(p>u.world.zw) {return 0xffffffffu;}
 let xy=vec2<u32>(clamp(floor((p-u.grid.xy)*u.grid.zw),vec2(0.),vec2<f32>(u.counts.yz)-vec2(1.)));
 return xy.y*u.counts.y+xy.x;
}
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
    let p=array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));return vec4(p[i],0.,1.);
}
fn cross(a:vec2<f32>,b:vec2<f32>)->f32{return a.x*b.y-a.y*b.x;}
fn contains(s:Primitive,p:vec2<f32>)->bool {
    if (s.tag.x==1u || s.tag.x==3u) && (any(p<s.bounds.xy) || any(p>s.bounds.zw)) {return false;}
    if s.tag.x==0u {
        let d=s.a.zw-s.a.xy; let l=dot(d,d);var t=0.;
        if l>0. {t=clamp(dot(p-s.a.xy,d)/l,0.,1.);}
        var r=s.b.x;if s.tag.w==1u {r=0.75/u.camera.z;}
        return distance(p,s.a.xy+t*d)<=r;
    }
    if s.tag.x==2u {
        let q=p-s.a.xy;let angle=atan2(q.y,q.x);
        let raw=(angle-s.b.x)*s.b.z;
        let sweep=raw-floor(raw/6.28318530718)*6.28318530718;
        var w=s.a.w;if s.tag.w==1u {w=0.75/u.camera.z;}
        return abs(length(q)-s.a.z)<=w && (s.b.y>=6.283185 || sweep<=s.b.y);
    }
    if s.tag.x==3u {
        let bin_count=u32(s.b.x);
        let bin=min(u32(clamp(floor((p.y-s.a.y)*s.a.z),0.,f32(bin_count-1u))),bin_count-1u);
        let left=p.x<s.b.z;
        var header_index=s.tag.z+s.tag.w+bin;
        if left {header_index=u32(s.b.w)+bin;}
        let header=points[header_index];
        let start=u32(header.x);let count=u32(header.y);
        if s.a.x>0. {
            var winding=0i;
            for(var i=0u;i<count;i++) {
                let a=points[start+i*2u];let b=points[start+i*2u+1u];
                if (left && min(a.x,b.x)>p.x) || (!left && max(a.x,b.x)<p.x) {break;}
                let side=select(1.,-1.,left);
                if a.y<=p.y && b.y>p.y && cross(b-a,p-a)*side>0. {winding++;}
                if a.y>p.y && b.y<=p.y && cross(b-a,p-a)*side<0. {winding--;}
            }
            return winding!=0i;
        }
        var odd=false;
        for(var i=0u;i<count;i++) {
            let a=points[start+i*2u];let b=points[start+i*2u+1u];
            if (left && min(a.x,b.x)>p.x) || (!left && max(a.x,b.x)<p.x) {break;}
            if (a.y>p.y)!=(b.y>p.y) {
                let crossing=(b.x-a.x)*(p.y-a.y)/(b.y-a.y)+a.x;
                if select(p.x<crossing,p.x>crossing,left) {odd=!odd;}
            }
        }
        return odd;
    }
    if s.a.x>0. {
        var winding=0i;
        for(var i=0u;i<s.tag.w;i++) {
            let a=points[s.tag.z+i];let b=points[s.tag.z+(i+1u)%s.tag.w];
            if a.y<=p.y && b.y>p.y && cross(b-a,p-a)>0. {winding++;}
            if a.y>p.y && b.y<=p.y && cross(b-a,p-a)<0. {winding--;}
        }
        return winding!=0i;
    }
    var odd=false;
    for(var i=0u;i<s.tag.w;i++) {
        let a=points[s.tag.z+i];let b=points[s.tag.z+(i+1u)%s.tag.w];
        if (a.y>p.y)!=(b.y>p.y) {if p.x<(b.x-a.x)*(p.y-a.y)/(b.y-a.y)+a.x {odd=!odd;}}
    }
    return odd;
}
fn object_material(o:Object,p:vec2<f32>)->bool {
    var material=false;
    for(var j=o.tag.x;j<o.tag.y;j++) {let s=shapes[j];if contains(s,p) {material=s.tag.y==1u;}}
    return material;
}
fn unpack_color(x:u32)->vec3<f32> {
    return vec3(f32((x>>16u)&255u),f32((x>>8u)&255u),f32(x&255u))/255.;
}
fn object_hit(o:Object,p:vec2<f32>)->bool {
    if o.style.y==1u {
        let e=1./u.camera.z;
        let a=object_material(o,p+vec2(e,0.));let b=object_material(o,p-vec2(e,0.));
        let c=object_material(o,p+vec2(0.,e));let d=object_material(o,p-vec2(0.,e));
        return a!=b || c!=d;
    }
    return object_material(o,p);
}
fn halo(color:vec3<f32>)->vec3<f32> {
    let inverse=vec3(1.)-color;
    if length(inverse-color)<0.5 {
        if dot(color,vec3(0.299,0.587,0.114))>0.5 {return vec3(0.);}
        return vec3(1.);
    }
    return inverse;
}
fn sample_scene(p:vec2<f32>)->vec3<f32> {
    var color=vec3(0.055,0.072,0.085);var layer=0u;var covered=false;var lcolor=vec3(0.);
    // Two ordered streams from the same immutable bins. No per-frame index
    // rebuild, and no selected-last compositing that would reorder Dark/Clear.
    let moving=any(u.preview.xy!=vec2(0.));
    let c=cell(p);let shifted=cell(p-u.preview.xy);
    var cursor=0u;var end=0u;var moved_cursor=0u;var moved_end=0u;
    if c!=0xffffffffu {cursor=bins[c];end=bins[c+1u];}
    if moving && shifted!=0xffffffffu {moved_cursor=bins[shifted];moved_end=bins[shifted+1u];}
    loop {
        if moving {
            loop {if cursor>=end {break;} if selected[bins[cursor]]==0u {break;} cursor++;}
            loop {if moved_cursor>=moved_end {break;} if selected[bins[moved_cursor]]!=0u {break;} moved_cursor++;}
        }
        var stationary=u.counts.x;var translated=u.counts.x;
        if cursor<end {stationary=bins[cursor];}
        if moved_cursor<moved_end {translated=bins[moved_cursor];}
        let i=min(stationary,translated);
        if i==u.counts.x {break;}
        if stationary==i {cursor++;}
        if translated==i {moved_cursor++;}
        let o=objects[i];if o.tag.w==0u {continue;}
        if o.style.y!=0u && o.tag.z==0u {continue;}
        if o.tag.w!=layer {if covered {color=mix(color,lcolor,0.90);}layer=o.tag.w;covered=false;}
        var local_p=p;
        if selected[i]!=0u {local_p-=u.preview.xy;}
        var pad=0.;if o.style.y!=0u {pad=2./u.camera.z;}
        if local_p.x<o.bounds.x-pad || local_p.y<o.bounds.y-pad || local_p.x>o.bounds.z+pad || local_p.y>o.bounds.w+pad {continue;}
        if object_hit(o,local_p) {covered=o.tag.z==1u;if covered {lcolor=unpack_color(o.style.x);}}
    }
    if covered {color=mix(color,lcolor,0.90);}return color;
}
@fragment fn fs_main(@builtin(position) pos:vec4<f32>)->@location(0) vec4<f32> {
    let local=pos.xy-u.view.xy-u.view.zw*0.5;
    let p=u.camera.xy+vec2(local.x,-local.y)/u.camera.z;
    let q=0.25/u.camera.z;
    var edge=false;
    var color=(sample_scene(p+vec2(q,q))+sample_scene(p+vec2(q,-q))+sample_scene(p+vec2(-q,q))+sample_scene(p-vec2(q,q)))*0.25;
    if u.counts.w!=0u && all(p>=u.selection_bounds.xy) && all(p<=u.selection_bounds.zw) {
    let e=u.camera.w;
    let queries=array<vec2<f32>,4>(p+vec2(e,0.),p-vec2(e,0.),p+vec2(0.,e),p-vec2(0.,e));
    var cursors:array<u32,4>;
    var ends:array<u32,4>;
    for(var k=0u;k<4u;k++) {let c=cell(queries[k]-u.preview.xy);if c!=0xffffffffu {cursors[k]=bins[c];ends[k]=bins[c+1u];}}
    loop {
        var i=u.counts.x;
        for(var k=0u;k<4u;k++) {if cursors[k]<ends[k] {i=min(i,bins[cursors[k]]);}}
        if i==u.counts.x {break;}
        for(var k=0u;k<4u;k++) {if cursors[k]<ends[k] {if bins[cursors[k]]==i {cursors[k]++;}}}
        if selected[i]==0u {continue;}
        let o=objects[i];let p=p-u.preview.xy;let e=u.camera.w;
        if o.tag.w>0u && all(p>=o.bounds.xy-vec2(e*1.01)) && all(p<=o.bounds.zw+vec2(e*1.01)) {
          let a=object_material(o,p+vec2(e,0.));let b=object_material(o,p-vec2(e,0.));
          let c=object_material(o,p+vec2(0.,e));let d=object_material(o,p-vec2(0.,e));
          if a!=b || c!=d {edge=true;}
        }
    }
    }
    // One halo per pixel: the inverse colour is not idempotent, so several selected
    // display objects of one id must not cancel each other out.
    if edge {color=halo(color);}
    return vec4(color,1.);
}
