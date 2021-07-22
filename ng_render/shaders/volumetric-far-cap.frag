#version 450

layout(input_attachment_index = 0, set = 0, binding = 2) uniform subpassInput depth;

layout(push_constant) uniform LightBuffer {
    mat4 light_to_screen;
    vec4 sunlight_direction;
} light_buffer;

layout(location = 0) in vec4 ndc;
layout(location = 0) out vec4 fragColor;

void main() {
    vec4 pos = vec4(ndc.xy, subpassLoad(depth).r, 1);

    float facing_scale = -1.0;
    fragColor = vec4(facing_scale * vec3(1.0, 0.0, -1.0), 1.0);

// fragColor = vec3(0.0, 4.0, 0.0);

    // fragColor = gl_FrontFacing ? vec3(0.01, 0.02, 0.04) : vec3(0.04, 0.02, 0.02);
}
