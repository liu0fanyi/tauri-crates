//! Leptos DragDrop Utilities
//! 
//! Simple drag-and-drop for Leptos using mouse events.
//! Uses movement threshold to distinguish click from drag.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Drop target types
#[derive(Clone, Debug, PartialEq)]
pub enum DropTarget {
    /// Drop on an item (become child)
    Item(u32),
    /// Drop on a zone between items (parent_id, position)
    Zone(Option<u32>, i32),
    /// Drop on a file (path)
    File(String),
}

/// Computed drop action
#[derive(Clone, Default, Debug)]
pub struct DropAction {
    pub target: Option<DropTarget>,
}

/// DnD state signals
#[derive(Clone, Copy)]
pub struct DndSignals {
    pub dragging_id_read: ReadSignal<Option<u32>>,
    pub dragging_id_write: WriteSignal<Option<u32>>,
    pub drop_target_read: ReadSignal<Option<DropTarget>>,
    pub drop_target_write: WriteSignal<Option<DropTarget>>,
    pub drag_just_ended_read: ReadSignal<bool>,
    pub drag_just_ended_write: WriteSignal<bool>,
    /// Pending item id (mousedown but not yet dragging)
    pub pending_id_read: ReadSignal<Option<u32>>,
    pub pending_id_write: WriteSignal<Option<u32>>,
    /// Start position for movement detection
    pub start_x_read: ReadSignal<i32>,
    pub start_x_write: WriteSignal<i32>,
    pub start_y_read: ReadSignal<i32>,
    pub start_y_write: WriteSignal<i32>,
}

/// Movement threshold in pixels to start dragging
const DRAG_THRESHOLD_PX: i32 = 5;

pub fn create_dnd_signals() -> DndSignals {
    let (dragging_id_read, dragging_id_write) = signal(None::<u32>);
    let (drop_target_read, drop_target_write) = signal(None::<DropTarget>);
    let (drag_just_ended_read, drag_just_ended_write) = signal(false);
    let (pending_id_read, pending_id_write) = signal(None::<u32>);
    let (start_x_read, start_x_write) = signal(0i32);
    let (start_y_read, start_y_write) = signal(0i32);
    DndSignals {
        dragging_id_read,
        dragging_id_write,
        drop_target_read,
        drop_target_write,
        drag_just_ended_read,
        drag_just_ended_write,
        pending_id_read,
        pending_id_write,
        start_x_read,
        start_x_write,
        start_y_read,
        start_y_write,
    }
}

/// End drag operation
pub fn end_drag(dnd: &DndSignals) {
    if dnd.dragging_id_read.try_get_untracked().is_some() {
        dnd.dragging_id_write.set(None);
        dnd.drop_target_write.set(None);
        dnd.pending_id_write.set(None);
        dnd.drag_just_ended_write.set(true);
        
        if let Some(win) = web_sys::window() {
            let clear = dnd.drag_just_ended_write;
            // Capture checking closure to avoid panic in timeout
            let check = dnd.drag_just_ended_read;
            
            let cb = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
                if check.try_get_untracked().is_some() {
                    clear.set(false);
                }
            });
            let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(cb.as_ref().unchecked_ref(), 100);
            cb.forget();
        }
    }
}

/// Create mousedown handler for draggable items
/// Records pending drag with start position
pub fn make_on_mousedown(dnd: DndSignals, item_id: u32) -> impl Fn(web_sys::MouseEvent) + Copy + 'static {
    move |ev: web_sys::MouseEvent| {
        if ev.button() == 0 {
            // Ignore if target is input or button
            if let Some(target) = ev.target() {
                if target.dyn_ref::<web_sys::HtmlInputElement>().is_some() { return; }
                if target.dyn_ref::<web_sys::HtmlButtonElement>().is_some() { return; }
            }
            // Record pending drag with position
            dnd.pending_id_write.set(Some(item_id));
            dnd.start_x_write.set(ev.client_x());
            dnd.start_y_write.set(ev.client_y());
        }
    }
}

/// Create mousemove handler for document - starts drag if moved enough
pub fn bind_global_mousemove(dnd: DndSignals) {
    let _ = window_event_listener(leptos::ev::mousemove, move |ev: web_sys::MouseEvent| {
        // Use try_get_untracked to avoid panic if signal is disposed
        if let Some(pending) = dnd.pending_id_read.try_get_untracked() {
             // If we have a pending drag and haven't started dragging yet
            if pending.is_some() && dnd.dragging_id_read.try_get_untracked().flatten().is_none() {
                if let (Some(start_x), Some(start_y)) = (dnd.start_x_read.try_get_untracked(), dnd.start_y_read.try_get_untracked()) {
                    let dx = (ev.client_x() - start_x).abs();
                    let dy = (ev.client_y() - start_y).abs();
                    
                    // Start dragging if moved beyond threshold
                    if dx > DRAG_THRESHOLD_PX || dy > DRAG_THRESHOLD_PX {
                        dnd.dragging_id_write.set(pending);
                    }
                }
            }
        }
    });
}

/// Create mouseenter handler for items (become child target)
pub fn make_on_item_mouseenter(dnd: DndSignals, item_id: u32) -> impl Fn(web_sys::MouseEvent) + Copy + 'static {
    move |_ev: web_sys::MouseEvent| {
        if dnd.dragging_id_read.get_untracked().is_some() {
            let dragging = dnd.dragging_id_read.get_untracked().unwrap();
            // Don't allow dropping on self
            if dragging != item_id {
                dnd.drop_target_write.set(Some(DropTarget::Item(item_id)));
            }
        }
    }
}

/// Create mouseenter handler for files
pub fn make_on_file_mouseenter(dnd: DndSignals, path: String) -> impl Fn(web_sys::MouseEvent) + 'static {
    move |_ev: web_sys::MouseEvent| {
        if dnd.dragging_id_read.get_untracked().is_some() {
            dnd.drop_target_write.set(Some(DropTarget::File(path.clone())));
        }
    }
}

/// Create mouseenter handler for zones
pub fn make_on_zone_mouseenter(dnd: DndSignals, parent_id: Option<u32>, position: i32) -> impl Fn(web_sys::MouseEvent) + Copy + 'static {
    move |_ev: web_sys::MouseEvent| {
        if dnd.dragging_id_read.get_untracked().is_some() {
            dnd.drop_target_write.set(Some(DropTarget::Zone(parent_id, position)));
        }
    }
}

/// Create mouseleave handler
pub fn make_on_mouseleave(dnd: DndSignals) -> impl Fn(web_sys::MouseEvent) + Copy + 'static {
    move |_ev: web_sys::MouseEvent| {
        if dnd.dragging_id_read.get_untracked().is_some() {
            dnd.drop_target_write.set(None);
        }
    }
}

/// Bind global mouseup handler for drop detection
pub fn bind_global_mouseup<F>(dnd: DndSignals, on_drop: F)
where
    F: Fn(u32, DropTarget) + Clone + 'static,
{
    let _ = window_event_listener(leptos::ev::mouseup, move |_ev: web_sys::MouseEvent| {
        // Use try_get_untracked to avoid panic
        let dragging_id = dnd.dragging_id_read.try_get_untracked().flatten();
        let drop_target = dnd.drop_target_read.try_get_untracked().flatten();
        
        // Check if signal system is still active (using a read signal as proxy)
        if dnd.dragging_id_read.try_get_untracked().is_some() {
             dnd.pending_id_write.set(None);
        }

        // If we were actually dragging (not just clicking)
        if let (Some(dragged), Some(target)) = (dragging_id, drop_target) {
            end_drag(&dnd);
            on_drop(dragged, target);
        } else {
            // Not dragging - just end any pending state
            end_drag(&dnd);
            // Click event will fire naturally on the element
        }
    });

    // Also bind global mousemove
    bind_global_mousemove(dnd);
}
