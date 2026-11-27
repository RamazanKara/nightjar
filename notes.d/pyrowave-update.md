## en
- PyroWave streams decode correctly on GPUs whose Vulkan subgroups are smaller than 16 threads; before, the decoder read dequantization data from the wrong planes. The stream format and the host's stream are unchanged.
