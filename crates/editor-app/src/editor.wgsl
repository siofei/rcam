struct Uniforms { view:vec4<f32>, camera:vec4<f32>, counts:vec4<u32> }
struct Object { tag:vec4<u32>, bounds:vec4<f32> }
struct Primitive { tag:vec4<u32>, a:vec4<f32>, b:vec4<f32> }
@group(0) @binding(0) var<uniform> u:Uniforms;
@group(0) @binding(1) var<storage,read> objects:array<Object>;
@group(0) @binding(2) var<storage,read> shapes:array<Primitive>;
@group(0) @binding(3) var<storage,read> points:array<vec2<f32>>;
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
    for(var i=0u;i<u.counts.x;i++) {
        let o=objects[i];if o.tag.w==0u {continue;}
        if o.tag.w!=layer {if covered {color=mix(color,palette[(layer-1u)%4u],0.90);}layer=o.tag.w;covered=false;}
        if p.x<o.bounds.x || p.y<o.bounds.y || p.x>o.bounds.z || p.y>o.bounds.w {continue;}
        if object_material(o,p) {covered=o.tag.z==1u;}
    }
    if covered {color=mix(color,palette[(layer-1u)%4u],0.90);}return color;
}
@fragment fn fs_main(@builtin(position) pos:vec4<f32>)->@location(0) vec4<f32> {
    let local=pos.xy-u.view.xy-u.view.zw*0.5;
    let p=u.camera.xy+vec2(local.x,-local.y)/u.camera.z;
    let q=0.25/u.camera.z;
    var color=(sample_scene(p+vec2(q,q))+sample_scene(p+vec2(q,-q))+sample_scene(p+vec2(-q,q))+sample_scene(p-vec2(q,q)))*0.25;
    if u.counts.y>0u {
        let o=objects[u.counts.y-1u];let e=1.5/u.camera.z;
        if o.tag.w>0u && all(p>=o.bounds.xy-vec2(e)) && all(p<=o.bounds.zw+vec2(e)) {
        let a=object_material(o,p+vec2(e,0.));let b=object_material(o,p-vec2(e,0.));
        let c=object_material(o,p+vec2(0.,e));let d=object_material(o,p-vec2(0.,e));
        if a!=b || c!=d {color=vec3(1.,0.72,0.20);}
        }
    }
    return vec4(color,1.);
}
