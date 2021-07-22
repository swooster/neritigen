#version 450

layout(push_constant) uniform LightBuffer {
    mat4 light_to_screen;
    vec4 sunlight_direction;
} light_buffer;

layout(location = 0) in vec4 ndc;
layout(location = 0) out vec4 fragColor;

void main() {
    float facing_scale = 2*float(gl_FrontFacing) - 1.0;
    fragColor = vec4(facing_scale * vec3(1.0, 0.0, -1.0), 1.0);

    // fragColor = gl_FrontFacing ? vec4(0.01, 0.02, 0.04, 1.0) : vec4(0.04, 0.02, 0.02, 1.0);
}
