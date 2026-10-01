struct Uniforms { view:vec4<f32>, camera:vec4<f32>, counts:vec4<u32>, preview:vec4<f32> }
struct Object { tag:vec4<u32>, bounds:vec4<f32>, style:vec4<u32> }
struct Primitive { tag:vec4<u32>, a:vec4<f32>, b:vec4<f32>, bounds:vec4<f32> }
@group(0) @binding(0) var<uniform> u:Uniforms;
@group(0) @binding(1) var<storage,read> objects:array<Object>;
@group(0) @binding(2) var<storage,read> shapes:array<Primitive>;
@group(0) @binding(3) var<storage,read> points:array<vec2<f32>>;
@group(0) @binding(4) var<storage,read> selected:array<u32>;
@vertex fn vs_main(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
    let p=array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));return vec4(p[i],0.,1.);
}
fn cross(a:vec2<f32>,b:vec2<f32>)->f32{return a.x*b.y-a.y*b.x;}
fn contains(s:Primitive,p:vec2<f32>)->bool {
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
    for(var i=0u;i<u.counts.x;i++) {
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
    for(var i=0u;i<u.counts.x;i++) {
        if selected[i]==0u {continue;}
        let o=objects[i];let p=p-u.preview.xy;let e=u.camera.w;
        if o.tag.w>0u && all(p>=o.bounds.xy-vec2(e*1.01)) && all(p<=o.bounds.zw+vec2(e*1.01)) {
        let a=object_material(o,p+vec2(e,0.));let b=object_material(o,p-vec2(e,0.));
        let c=object_material(o,p+vec2(0.,e));let d=object_material(o,p-vec2(0.,e));
        if a!=b || c!=d {edge=true;}
        }
    }
    // One halo per pixel: the inverse colour is not idempotent, so several selected
    // display objects of one id must not cancel each other out.
    if edge {color=halo(color);}
    return vec4(color,1.);
}
