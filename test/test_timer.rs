use td_revent::*;
use std::fmt;

static mut S_COUNT: u32 = 0;
static mut S_DEL_TIMER: u32 = 0;

//timer return no success(0) will no be repeat
fn time_callback(
    ev: &mut EventLoop,
    timer: u32,
    data: Option<&mut CellAny>,
) -> (RetValue, u64) {
    if data.is_none() {
        println!("data is none");
        let count = unsafe {
            S_COUNT = S_COUNT + 1;
            S_COUNT
        };
        if count >= 5 {
            ev.shutdown();
        }
    } else {
        let cellany = data.unwrap();
        let obj = any_to_mut!(cellany, Point);
        obj.y = obj.y + 1;
        if obj.y >= 25 {
            return (RetValue::OK, 0);
        }
        println!("callback {:?}", obj);
        return (RetValue::CONTINUE, 10);
    }

    if unsafe { S_DEL_TIMER == timer } {
        return (RetValue::OVER, 0);
    }
    (RetValue::OK, 0)
}

#[derive(Default, Debug, Clone)]
struct Point {
    x: i32,
    y: i32,
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
    let mut event_loop: EventLoop = EventLoop::new().unwrap();
    let p = Point { x: 10, y: 20 };

    event_loop.add_timer(EventEntry::new_timer(
        100,
        false,
        Some(time_callback),
        Some(Box::new(p)),
    ));
    unsafe {
        S_DEL_TIMER =
            event_loop.add_timer(EventEntry::new_timer(150, true, Some(time_callback), None));
    }
    event_loop.add_timer(EventEntry::new_timer(200, true, Some(time_callback), None));
    event_loop.run().unwrap();
    assert!(unsafe { S_COUNT } == 5);
}
