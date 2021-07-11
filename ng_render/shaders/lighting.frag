#version 450

layout(input_attachment_index = 0, set = 0, binding = 0) uniform subpassInput inputColor;

layout(location = 0) out vec3 fragColor;

void main() {
    fragColor = subpassLoad(inputColor).rgb;
}
