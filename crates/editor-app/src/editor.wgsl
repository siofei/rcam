struct Uniforms { view:vec4<f32>, camera:vec4<f32>, counts:vec4<u32>, preview:vec4<f32>, grid:vec4<f32>, world:vec4<f32> }
struct Object { tag:vec4<u32>, bounds:vec4<f32> }
struct Primitive { tag:vec4<u32>, a:vec4<f32>, b:vec4<f32> }
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
    if s.tag.x==0u {
        let d=s.a.zw-s.a.xy; let l=dot(d,d);var t=0.;
        if l>0. {t=clamp(dot(p-s.a.xy,d)/l,0.,1.);}
        return distance(p,s.a.xy+t*d)<=s.b.x;
    }
    if s.tag.x==2u {
        let q=p-s.a.xy;let angle=atan2(q.y,q.x);
        let raw=(angle-s.b.x)*s.b.z;
        let sweep=raw-floor(raw/6.28318530718)*6.28318530718;
        return abs(length(q)-s.a.z)<=s.a.w && (s.b.y>=6.283185 || sweep<=s.b.y);
    }
    var winding=0i;var odd=false;
    for(var i=0u;i<s.tag.w;i++) {
        let a=points[s.tag.z+i];let b=points[s.tag.z+(i+1u)%s.tag.w];
        if a.y<=p.y && b.y>p.y && cross(b-a,p-a)>0. {winding++;}
        if a.y>p.y && b.y<=p.y && cross(b-a,p-a)<0. {winding--;}
        if (a.y>p.y)!=(b.y>p.y) {if p.x<(b.x-a.x)*(p.y-a.y)/(b.y-a.y)+a.x {odd=!odd;}}
    }
    if s.a.x>0. {return winding!=0i;}return odd;
}
fn object_material(o:Object,p:vec2<f32>)->bool {
    var material=false;
    for(var j=o.tag.x;j<o.tag.y;j++) {let s=shapes[j];if contains(s,p) {material=s.tag.y==1u;}}
    return material;
}
fn sample_scene(p:vec2<f32>)->vec3<f32> {
    var color=vec3(0.055,0.072,0.085);var layer=0u;var covered=false;
    let palette=array<vec3<f32>,4>(vec3(0.28,0.77,0.66),vec3(0.39,0.64,0.90),vec3(0.80,0.52,0.81),vec3(0.88,0.70,0.39));
    let c=cell(p);
    if c==0xffffffffu {return color;}
    for(var cursor=bins[c];cursor<bins[c+1u];cursor++) {
        let i=bins[cursor];
        let o=objects[i];if o.tag.w==0u {continue;}
        if o.tag.w!=layer {if covered {color=mix(color,palette[(layer-1u)%4u],0.90);}layer=o.tag.w;covered=false;}
        var local_p=p;
        if selected[i]!=0u {local_p-=u.preview.xy;}
        if local_p.x<o.bounds.x || local_p.y<o.bounds.y || local_p.x>o.bounds.z || local_p.y>o.bounds.w {continue;}
        if object_material(o,local_p) {covered=o.tag.z==1u;}
    }
    if covered {color=mix(color,palette[(layer-1u)%4u],0.90);}return color;
}
@fragment fn fs_main(@builtin(position) pos:vec4<f32>)->@location(0) vec4<f32> {
    let local=pos.xy-u.view.xy-u.view.zw*0.5;
    let p=u.camera.xy+vec2(local.x,-local.y)/u.camera.z;
    let q=0.25/u.camera.z;
    var color=(sample_scene(p+vec2(q,q))+sample_scene(p+vec2(q,-q))+sample_scene(p+vec2(-q,q))+sample_scene(p-vec2(q,q)))*0.25;
    let e=1.5/u.camera.z;
    let queries=array<vec2<f32>,4>(p+vec2(e,0.),p-vec2(e,0.),p+vec2(0.,e),p-vec2(0.,e));
    var cursors:array<u32,4>;
    var ends:array<u32,4>;
    for(var k=0u;k<4u;k++) {let c=cell(queries[k]);if c!=0xffffffffu {cursors[k]=bins[c];ends[k]=bins[c+1u];}}
    loop {
        var i=u.counts.x;
        for(var k=0u;k<4u;k++) {if cursors[k]<ends[k] {i=min(i,bins[cursors[k]]);}}
        if i==u.counts.x {break;}
        for(var k=0u;k<4u;k++) {if cursors[k]<ends[k] {if bins[cursors[k]]==i {cursors[k]++;}}}
        if selected[i]==0u {continue;}
        let o=objects[i];let p=p-u.preview.xy;let e=1.5/u.camera.z;
        if o.tag.w>0u && all(p>=o.bounds.xy-vec2(e)) && all(p<=o.bounds.zw+vec2(e)) {
          let a=object_material(o,p+vec2(e,0.));let b=object_material(o,p-vec2(e,0.));
          let c=object_material(o,p+vec2(0.,e));let d=object_material(o,p-vec2(0.,e));
          if a!=b || c!=d {color=vec3(1.,0.72,0.20);}
        }
    }
    return vec4(color,1.);
}
