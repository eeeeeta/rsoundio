use std::os::raw::{c_int, c_double, c_void};
use std::ptr;

use ffi;
use base::*;

extern "C" fn write_wrapper<W>(raw_out: *mut ffi::SoundIoOutStream, min: c_int, max: c_int)
    where W: Fn(OutStream, i32, i32)
{
    let out = OutStream::new(raw_out);
    let callbacks_ptr = unsafe { (*out.stream).userdata as *const Box<OutStreamCallbacks> };
    let callbacks: &Box<OutStreamCallbacks> = unsafe { &*callbacks_ptr };
    callbacks.write.as_ref().map(|ref f| f(out, min as i32, max as i32));
}

extern "C" fn underflow_wrapper<U>(raw_out: *mut ffi::SoundIoOutStream)
    where U: Fn(OutStream)
{
    let out = OutStream::new(raw_out);
    let callbacks_ptr = unsafe { (*out.stream).userdata as *const Box<OutStreamCallbacks> };
    let callbacks: &Box<OutStreamCallbacks> = unsafe { &*callbacks_ptr };
    callbacks.underflow.as_ref().map(|ref f| f(out));
}

extern "C" fn error_wrapper<E>(raw_out: *mut ffi::SoundIoOutStream, error: ffi::SioError)
    where E: Fn(OutStream, ffi::SioError)
{
    let out = OutStream::new(raw_out);
    let callbacks_ptr = unsafe { (*out.stream).userdata as *const Box<OutStreamCallbacks> };
    let callbacks: &Box<OutStreamCallbacks> = unsafe { &*callbacks_ptr };
    callbacks.error.as_ref().map(|ref f| f(out, error));
}

struct OutStreamCallbacks<'a> {
    write: Option<Box<Fn(OutStream, i32, i32) + 'a>>,
    underflow: Option<Box<Fn(OutStream) + 'a>>,
    error: Option<Box<Fn(OutStream, ffi::SioError) + 'a>>,
}
impl<'a> Default for OutStreamCallbacks<'a> {
    fn default() -> Self {
        OutStreamCallbacks {
            write: None,
            underflow: None,
            error: None,
        }
    }
}
impl<'a> Drop for OutStreamCallbacks<'a> {
    fn drop(&mut self) {}
}

pub struct OutStream<'a> {
    stream: *mut ffi::SoundIoOutStream,
    callbacks: Box<OutStreamCallbacks<'a>>,
}
impl<'a> OutStream<'a> {
    pub fn new(raw_stream: *mut ffi::SoundIoOutStream) -> Self {
        let callbacks = Box::new(OutStreamCallbacks::default());
        OutStream {
            stream: raw_stream,
            callbacks: callbacks,
        }
    }

    pub fn open(&self) -> Option<ffi::SioError> {
        match unsafe { ffi::soundio_outstream_open(self.stream) } {
            ffi::SioError::None => None,
            err @ _ => Some(err),
        }
    }

    pub fn start(&self) -> Option<ffi::SioError> {
        match unsafe { ffi::soundio_outstream_start(self.stream) } {
            ffi::SioError::None => None,
            err @ _ => Some(err),
        }
    }

    pub fn register_write_callback<W>(&mut self, callback: Box<W>)
        where W: Fn(OutStream, i32, i32) + 'a
    {
        // stored box reference to callback closure
        self.callbacks.write = Some(callback);
        unsafe {
            // register wrapper for write_callback
            (*self.stream).write_callback = Some(write_wrapper::<W>);
            // store reference to callbacks struct in userdata pointer
            (*self.stream).userdata =
                &self.callbacks as *const Box<OutStreamCallbacks> as *mut c_void
        }
    }

    pub fn register_underflow_callback<U>(&mut self, callback: Box<U>)
        where U: Fn(OutStream) + 'a
    {
        self.callbacks.underflow = Some(callback);
        unsafe {
            // register wrapper for write_callback
            (*self.stream).underflow_callback = Some(underflow_wrapper::<U>);
            // store reference to callbacks struct in userdata pointer
            (*self.stream).userdata =
                &self.callbacks as *const Box<OutStreamCallbacks> as *mut c_void
        }
    }

    pub fn register_error_callback<E>(&mut self, callback: Box<E>)
        where E: Fn(OutStream, ffi::SioError) + 'a
    {
        self.callbacks.error = Some(callback);
        unsafe {
            // register wrapper for write_callback
            (*self.stream).error_callback = Some(error_wrapper::<E>);
            // store reference to callbacks struct in userdata pointer
            (*self.stream).userdata =
                &self.callbacks as *const Box<OutStreamCallbacks> as *mut c_void
        }
    }

    pub fn write_stream(&self,
                        min_frame_count: i32,
                        buffers: &Vec<Vec<f32>>)
                        -> Result<i32, ffi::SioError> {
        let channel_count = self.get_layout().channel_count();
        // check if buffer contains frames for all channels
        if buffers.len() < channel_count as usize {
            return Err(ffi::SioError::Invalid);
        }
        // check if there are at least min_frame_count frames for all channels
        if !buffers.iter().map(|c| c.len()).all(|l| l >= min_frame_count as usize) {
            return Err(ffi::SioError::Invalid);
        }

        // assuming that every channel buffer has the same length
        let mut frame_count = buffers[0].len() as c_int;
        let mut raw_areas: *mut ffi::SoundIoChannelArea = ptr::null_mut();
        let actual_frame_count = try!(self.begin_write(&mut raw_areas, &frame_count));
        let areas = unsafe { ::std::slice::from_raw_parts_mut(raw_areas, channel_count as usize) };
        for idx in 0..actual_frame_count as usize {
            for channel in 0..channel_count as usize {
                let area = areas[channel];
                let addr = (area.ptr as usize + area.step as usize * idx) as *mut f32;
                unsafe { *addr = buffers[channel][idx] }
            }
        }
        self.end_write().map_or(Ok(actual_frame_count), |err| Err(err))
    }

    pub fn begin_write(&self,
                       areas: *mut *mut ffi::SoundIoChannelArea,
                       frame_count: *mut c_int)
                       -> Option<ffi::SioError> {
        match unsafe { ffi::soundio_outstream_begin_write(self.stream, areas, frame_count) } {
            ffi::SioError::None => None,
            err @ _ => Some(err),
        }
    }

    pub fn end_write(&self) -> Option<ffi::SioError> {
        match unsafe { ffi::soundio_outstream_end_write(self.stream) } {
            ffi::SioError::None => None,
            err @ _ => Some(err),
        }
    }

    pub fn clear_buffer(&self) -> Option<ffi::SioError> {
        match unsafe { ffi::soundio_outstream_clear_buffer(self.stream) } {
            ffi::SioError::None => None,
            err @ _ => Some(err),
        }
    }

    pub fn pause(&self, pause: bool) -> Option<ffi::SioError> {
        let pause_c_bool = match pause {
            true => 1u8,
            false => 0u8,
        };
        match unsafe { ffi::soundio_outstream_pause(self.stream, pause_c_bool) } {
            ffi::SioError::None => None,
            err @ _ => Some(err),
        }
    }

    pub fn get_latency(&self) -> Result<f64, ffi::SioError> {
        let mut latency = 0.0f64;
        match unsafe {
            ffi::soundio_outstream_get_latency(self.stream, &mut latency as *mut c_double)
        } {
            ffi::SioError::None => Ok(latency),
            err @ _ => Err(err),
        }

    }

    pub fn current_format(&self) -> Result<ffi::SioFormat, ffi::SioError> {
        match unsafe { (*self.stream).format } {
            ffi::SioFormat::Invalid => Err(ffi::SioError::Invalid),
            fmt @ _ => Ok(fmt),
        }
    }

    pub fn get_layout(&self) -> ChannelLayout {
        ChannelLayout::new(unsafe { &(*self.stream).layout })
    }

    pub fn get_sample_rate(&self) -> i32 {
        unsafe { (*self.stream).sample_rate as i32 }
    }

    pub fn get_device(&self) -> Device {
        let dev = Device::new(unsafe { (*self.stream).device });
        dev.inc_ref();
        dev
    }

    pub fn destroy(&self) {
        unsafe { ffi::soundio_outstream_destroy(self.stream) }
    }
}
impl<'a> Drop for OutStream<'a> {
    fn drop(&mut self) {
        // TODO: call destroy manually.
        // OutStream will get dropped each time a new
        // struct is created from the same *mut pointer.
    }
}