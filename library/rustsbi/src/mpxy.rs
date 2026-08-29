use sbi_spec::binary::{SbiRet, SharedPtr};

/// SBI Message Proxy (MPXY) support extension.
pub trait Mpxy {
    /// Get the shared memory size in bytes.
    fn get_shmem_size(&self) -> usize;

    /// Set shared memory for the calling hart.
    fn set_shmem(&self, shmem: SharedPtr<u8>, flags: usize) -> SbiRet;

    /// Get channel IDs into shared memory.
    fn get_channel_ids(&self, start_index: u32) -> SbiRet;

    /// Read channel attributes.
    fn read_attributes(
        &self,
        channel_id: u32,
        base_attribute_id: u32,
        attribute_count: u32,
        output: SharedPtr<u8>,
    ) -> SbiRet;

    /// Write channel attributes.
    fn write_attributes(
        &self,
        channel_id: u32,
        base_attribute_id: u32,
        attribute_count: u32,
        input: SharedPtr<u8>,
    ) -> SbiRet;

    /// Send a message and wait for its response.
    fn send_message_with_response(
        &self,
        channel_id: u32,
        message_id: u32,
        message_data_len: usize,
    ) -> SbiRet;

    /// Send a message without waiting for a response.
    fn send_message_without_response(
        &self,
        channel_id: u32,
        message_id: u32,
        message_data_len: usize,
    ) -> SbiRet;

    /// Get notification events for a channel.
    fn get_notification_events(&self, channel_id: u32) -> SbiRet;

    /// Function internal to macros. Do not use.
    #[doc(hidden)]
    #[inline]
    fn _rustsbi_probe(&self) -> usize {
        sbi_spec::base::UNAVAILABLE_EXTENSION.wrapping_add(1)
    }
}

impl<T: Mpxy> Mpxy for &T {
    #[inline]
    fn get_shmem_size(&self) -> usize {
        T::get_shmem_size(self)
    }

    #[inline]
    fn set_shmem(&self, shmem: SharedPtr<u8>, flags: usize) -> SbiRet {
        T::set_shmem(self, shmem, flags)
    }

    #[inline]
    fn get_channel_ids(&self, start_index: u32) -> SbiRet {
        T::get_channel_ids(self, start_index)
    }

    #[inline]
    fn read_attributes(
        &self,
        channel_id: u32,
        base_attribute_id: u32,
        attribute_count: u32,
        output: SharedPtr<u8>,
    ) -> SbiRet {
        T::read_attributes(self, channel_id, base_attribute_id, attribute_count, output)
    }

    #[inline]
    fn write_attributes(
        &self,
        channel_id: u32,
        base_attribute_id: u32,
        attribute_count: u32,
        input: SharedPtr<u8>,
    ) -> SbiRet {
        T::write_attributes(self, channel_id, base_attribute_id, attribute_count, input)
    }

    #[inline]
    fn send_message_with_response(
        &self,
        channel_id: u32,
        message_id: u32,
        message_data_len: usize,
    ) -> SbiRet {
        T::send_message_with_response(self, channel_id, message_id, message_data_len)
    }

    #[inline]
    fn send_message_without_response(
        &self,
        channel_id: u32,
        message_id: u32,
        message_data_len: usize,
    ) -> SbiRet {
        T::send_message_without_response(self, channel_id, message_id, message_data_len)
    }

    #[inline]
    fn get_notification_events(&self, channel_id: u32) -> SbiRet {
        T::get_notification_events(self, channel_id)
    }
}

impl<T: Mpxy> Mpxy for Option<T> {
    #[inline]
    fn get_shmem_size(&self) -> usize {
        self.as_ref().map_or(0, |inner| T::get_shmem_size(inner))
    }

    #[inline]
    fn set_shmem(&self, shmem: SharedPtr<u8>, flags: usize) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::set_shmem(inner, shmem, flags)
        })
    }

    #[inline]
    fn get_channel_ids(&self, start_index: u32) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::get_channel_ids(inner, start_index)
        })
    }

    #[inline]
    fn read_attributes(
        &self,
        channel_id: u32,
        base_attribute_id: u32,
        attribute_count: u32,
        output: SharedPtr<u8>,
    ) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::read_attributes(
                inner,
                channel_id,
                base_attribute_id,
                attribute_count,
                output,
            )
        })
    }

    #[inline]
    fn write_attributes(
        &self,
        channel_id: u32,
        base_attribute_id: u32,
        attribute_count: u32,
        input: SharedPtr<u8>,
    ) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::write_attributes(inner, channel_id, base_attribute_id, attribute_count, input)
        })
    }

    #[inline]
    fn send_message_with_response(
        &self,
        channel_id: u32,
        message_id: u32,
        message_data_len: usize,
    ) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::send_message_with_response(inner, channel_id, message_id, message_data_len)
        })
    }

    #[inline]
    fn send_message_without_response(
        &self,
        channel_id: u32,
        message_id: u32,
        message_data_len: usize,
    ) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::send_message_without_response(inner, channel_id, message_id, message_data_len)
        })
    }

    #[inline]
    fn get_notification_events(&self, channel_id: u32) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::get_notification_events(inner, channel_id)
        })
    }

    #[inline]
    fn _rustsbi_probe(&self) -> usize {
        match self {
            Some(_) => sbi_spec::base::UNAVAILABLE_EXTENSION.wrapping_add(1),
            None => sbi_spec::base::UNAVAILABLE_EXTENSION,
        }
    }
}
