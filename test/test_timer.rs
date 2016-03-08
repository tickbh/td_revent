use td_revent::*;
use std::fmt;
use std::ptr;

static mut s_count : i32 = 0;
static mut s_delTimer : u64 = 0;

//timer return no success(0) will no be repeat
fn time_callback(ev : &mut EventLoop, fd : u64, _ : EventFlags, data : *mut ()) -> i32 {
    let obj : *mut Point = data as *mut Point;
    if obj.is_null() {
        println!("data is null {:?}", data);
        let count = unsafe { s_count = s_count + 1; s_count };
        if count >= 5  {
            ev.shutdown();
        }
    } else {
        let obj : &mut Point = unsafe { &mut *obj };
        obj.y = obj.y+1;
        println!("callback {:?}", obj);
    }

    if unsafe { s_delTimer == fd } {
        return -1;
    }
    0
}

#[derive(Default, Debug, Clone)]
struct  Point   {
    x:  i32,
    y:  i32,
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "(--{}, {}--)", self.x, self.y)
    }
}

impl Drop for Point {
    fn drop(&mut self) {
        println!("drop point");
    }    
}

#[test]
pub fn test_timer() {
    println!("Starting TEST_TIMER");
    let mut event_loop : EventLoop = EventLoop::new().unwrap();
    let p = Point { x : 10, y : 20 };
    event_loop.add_timer(EventEntry::new_timer(100, false, Some(time_callback), Some( &p as *const _ as *mut () )));
    unsafe {
        s_delTimer = event_loop.add_timer(EventEntry::new_timer(150, true, Some(time_callback), Some( &p as *const _ as *mut () )));
    }
    event_loop.add_timer(EventEntry::new_timer(200, true, Some(time_callback), Some( ptr::null_mut() )));
    event_loop.run().unwrap();
    assert!(p.y == 22);
    assert!(unsafe { s_count } == 5);
}