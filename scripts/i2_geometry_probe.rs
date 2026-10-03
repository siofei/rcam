// Test-only semantic query probe; retained from independent A review.
use editor_core::*;
use editor_core::edit::SelectionGroup;
use editor_core::hit_test::selection_geometry::{calculate,CompositeMaterial};
use std::io::{self,BufRead};
fn main(){for input in io::stdin().lock().lines(){let line=input.unwrap();let mut it=line.split_whitespace();
 let mut doc=SemanticDocument{id:"independent".into(),unit:"mm".into(),format:SemanticFormat{integer:4,decimal:6,leading_zero_omission:true,absolute:true},layers:vec![SemanticLayer{id:"l".into(),objects:vec![]}],apertures:vec![],source:SourceMetadata::default(),block_definitions:vec![]};
 let head=it.next().unwrap();let circle=head=="C";let count:usize=if circle{it.next().unwrap()}else{head}.parse().unwrap();
 for j in 0..count {let exposure=if it.next().unwrap()=="D"{Exposure::Dark}else{Exposure::Clear};if circle{let r:f64=it.next().unwrap().parse().unwrap();let x:f64=it.next().unwrap().parse().unwrap();let y:f64=it.next().unwrap().parse().unwrap();let id=format!("a{j}");doc.apertures.push(ApertureDefinition{id:id.clone(),source_dcode:10+j as i32,shape:ApertureShape::Circle{diameter_mm:2.*r,hole_diameter_mm:None}});doc.layers[0].objects.push(SemanticObject{object_id:format!("o{j}"),geometry:SemanticGeometry::Flash{center:MmPoint::new(x,y),aperture_id:id,transform:LocalTransform::default()},exposure,origin:ObjectOrigin::Imported{command_index:j}});continue;}let n:usize=it.next().unwrap().parse().unwrap();let mut points=vec![];for _ in 0..n {let x:f64=it.next().unwrap().parse().unwrap();let y:f64=it.next().unwrap().parse().unwrap();points.push(MmPoint::new(x,y));}
 let edges=points.iter().zip(points.iter().cycle().skip(1)).take(n).map(|(a,b)|RegionEdge::Line{start:*a,end:*b}).collect();
 doc.layers[0].objects.push(SemanticObject{object_id:format!("o{j}"),geometry:SemanticGeometry::Region{contours:vec![RegionContour{role:RegionRole::Solid,edges}]},exposure,origin:ObjectOrigin::Imported{command_index:j}});
 }
 let groups=vec![SelectionGroup{layer_id:"l".into(),object_ids:doc.layers[0].objects.iter().map(|o|o.object_id.clone()).collect()}];let before=doc.clone();
 match calculate(&doc,&groups,1e-4,||false){Ok(q)=>match q.material{CompositeMaterial::Ready{area_mm2,perimeter_mm,centroid_mm,centroid_error_mm,area_error_mm2,perimeter_error_mm,..}=>println!("READY {area_mm2:.17e} {perimeter_mm:.17e} {:.17e} {:.17e} {centroid_error_mm:.17e} {area_error_mm2:.17e} {perimeter_error_mm:.17e}",centroid_mm.x_mm,centroid_mm.y_mm),CompositeMaterial::ZeroArea=>println!("ZERO")},Err(e)=>println!("ERR {e:?}")};assert_eq!(before,doc);
}}
