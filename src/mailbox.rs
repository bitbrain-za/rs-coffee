// pub trait Mailbox {
//     type Message;
//     fn push(&mut self, message: Self::Message);
//     fn pop(&mut self) -> Option<Self::Message>;
//     fn drain(&mut self) -> Vec<Self::Message>;
// }

pub struct Mailbox<T> {
    messages: Vec<T>,
}
