use td_revent::*;
use std::fmt;
use std::any::Any;

static mut S_COUNT : i32 = 0;
static mut S_DEL_TIMER : i32 = 0;

//timer return no success(0) will no be repeat
fn time_callback(ev : &mut EventLoop, fd : i32, _ : EventFlags, data : Option<&mut Box<Any>>) -> RetValue {
    if data.is_none() {
        println!("data is none");
        let count = unsafe { S_COUNT = S_COUNT + 1; S_COUNT };
        if count >= 5  {
            ev.shutdown();
        }
    } else {
        let obj = any_to_mut!(data, Point);
        obj.y = obj.y + 1;
        println!("callback {:?}", obj);
    }

    if unsafe { S_DEL_TIMER == fd } {
        return RetValue::OVER;
    }
    RetValue::OK
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

    event_loop.add_timer(EventEntry::new_timer(100, false, Some(time_callback), Some( Box::new(p) )));
    unsafe {
        S_DEL_TIMER = event_loop.add_timer(EventEntry::new_timer(150, true, Some(time_callback), None));
    }
    event_loop.add_timer(EventEntry::new_timer(200, true, Some(time_callback), None));
    event_loop.run().unwrap();
    assert!(unsafe { S_COUNT } == 5);
}