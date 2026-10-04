"""Frozen C synthetic native trajectory. Pure data; no execution or I/O."""

def steps(smoke=False):
    widget=lambda name:{'kind':'widget','name':name}
    prefs=lambda d=True,g=True,c='normal':{'kind':'c_preferences','value':{'drag_move':d,'grip_edit':g,'cursor':c}}
    steps=[{'kind':k} for k in ('c_new','b_import','b_camera','b_select_all')]
    # Actual production menu controls and persistence; helper restores baseline only.
    steps += [widget('menu-view'),widget('menu-interaction'),widget('interaction-drag'),widget('menu-view'),widget('menu-interaction'),widget('interaction-grip'),widget('menu-view'),widget('menu-interaction'),widget('cursor-small'),{'kind':'escape'},{'kind':'escape'},{'kind':'c_reload_preferences'},{'kind':'c_hover','from':[10.,1.]},{'kind':'c_hover_menu','name':'menu-file'},prefs(c='large_cross'),{'kind':'c_hover','from':[10.,1.]},prefs()]
    if not smoke:
     for d,g in [(False,False),(False,True),(True,False),(True,True)]:
      steps += [prefs(d,g),{'kind':'b_tool_select'},{'kind':'b_active_last'},
                {'kind':'b_select_one'}, {'kind':'click','from':[20.,4.]},
                {'kind':'drag','from':[-2.,-2.],'to':[22.,6.]},
                {'kind':'b_select_one'}, {'kind':'drag','from':[20.,4.],'to':[22.,5.]}]
      if d:steps += [{'kind':'undo'}]
      # Grip disabled + drag enabled falls through to ordinary movement.
      steps += [{'kind':'b_select_one'}, {'kind':'drag','from':[20.5,4.],'to':[22.,4.]}]
      if d or g:steps += [{'kind':'undo'}]
      steps += [{'kind':'b_modal','tool':'move'},widget('B-area'),widget('transform-apply'),{'kind':'undo'}]
     for style in ['normal','small_cross','large_cross']:
      steps += [prefs(c=style),{'kind':'c_hover','from':[10.,1.]},{'kind':'c_hover_menu','name':'menu-file'},widget('menu-view'),widget('menu-interaction'),{'kind':'escape'},{'kind':'escape'},{'kind':'zoom','from':[10.,1.],'factor':1.1},{'kind':'pan'}]
     steps += [prefs(),{'kind':'b_camera'},{'kind':'b_active_last'},{'kind':'b_select_one'}]
     for mouse,origin,target in [('drag',[20.,4.],[22.,5.]),('grip',[20.5,4.],[22.,4.])]:
      for cancel in ['none','escape','repeat','blur','gone','ime']:
       steps += [prefs(),{'kind':'drag','from':origin,'to':target,'switch_off':mouse,'cancel':cancel}]
       if cancel=='ime':steps += [{'kind':'ime_end'}]
     steps += [prefs(),{'kind':'c_reload_preferences'}]
     for size in [[820.,650.],[1000.,720.],[1440.,900.]]:steps += [{'kind':'c_resize','size':size},{'kind':'b_select_all'},{'kind':'c_hover','from':[10.,1.]}]
     # Concave Region plus selected-only Dark/Clear overlap, then actual Block UI.
     steps += [{'kind':k} for k in ('c_new','c_complex_import','c_complex_select','b_block_create')]+[widget('block-origin'),widget('adapter-area'),widget('adapter-apply'),widget('block-create'),{'kind':'b_block_place'},widget('block-local'),widget('adapter-bounds'),widget('adapter-apply'),widget('block-target'),widget('adapter-apply'),{'kind':'b_tool_select'}]
     # Existing overlapping native I1 workflow, with all historical meanings intact.
     from run_i1_native import steps as i1_steps
     steps += [prefs()]+[dict(step,kind='c_new') if step['kind']=='new' else step for step in i1_steps(False)]
    return steps
