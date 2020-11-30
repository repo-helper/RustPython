use crossbeam_utils::atomic::AtomicCell;
use num_bigint::BigInt;
use num_traits::FromPrimitive;
use rustpython_common::borrow::BorrowValue;
use std::fmt;

use crate::builtins::PyTypeRef;
use crate::builtins::{PyByteArray, PyBytes, PyFloat, PyInt, PyNone, PyStr};
use crate::pyobject::{
    PyObjectRc, PyObjectRef, PyRef, PyResult, PyValue, StaticType, TryFromObject, TypeProtocol,
};
use crate::VirtualMachine;

use crate::stdlib::ctypes::basics::PyCData;

pub const SIMPLE_TYPE_CHARS: &str = "cbBhHiIlLdfguzZqQ?";

#[pyclass(module = "_ctypes", name = "_SimpleCData", base = "PyCData")]
pub struct PySimpleType {
    _type_: String,
    value: AtomicCell<PyObjectRc>,
    __abstract__: bool,
}

impl fmt::Debug for PySimpleType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let value = unsafe { (*self.value.as_ptr()).to_string() };

        write!(
            f,
            "PySimpleType {{
            _type_: {},
            value: {},
        }}",
            self._type_.as_str(),
            value
        )
    }
}

fn set_primitive(_type_: &str, value: &PyObjectRc, vm: &VirtualMachine) -> PyResult<PyObjectRc> {
    match _type_ {
        "c" => {
            if value
                .clone()
                .downcast_exact::<PyBytes>(vm)
                .map(|v| v.len() == 1)
                .is_ok()
                || value
                    .clone()
                    .downcast_exact::<PyByteArray>(vm)
                    .map(|v| v.borrow_value().len() == 1)
                    .is_ok()
                || value
                    .clone()
                    .downcast_exact::<PyInt>(vm)
                    .map(|v| {
                        v.borrow_value().ge(&BigInt::from_i64(0).unwrap())
                            || v.borrow_value().le(&BigInt::from_i64(255).unwrap())
                    })
                    .is_ok()
            {
                Ok(value.clone())
            } else {
                Err(vm.new_type_error(
                    "one character bytes, bytearray or integer expected".to_string(),
                ))
            }
        }
        "u" => {
            if let Ok(b) = value
                .clone()
                .downcast_exact::<PyStr>(vm)
                .map(|v| v.as_ref().chars().count() == 1)
            {
                if b {
                    Ok(value.clone())
                } else {
                    Err(vm.new_type_error("one character unicode string expected".to_string()))
                }
            } else {
                Err(vm.new_type_error(format!(
                    "unicode string expected instead of {} instance",
                    value.class().name
                )))
            }
        }
        "b" | "h" | "H" | "i" | "I" | "l" | "q" | "L" | "Q" => {
            if value.clone().downcast_exact::<PyInt>(vm).is_ok() {
                Ok(value.clone())
            } else {
                Err(vm.new_type_error(format!(
                    "an integer is required (got type {})",
                    value.class().name
                )))
            }
        }
        "f" | "d" | "g" => {
            if value.clone().downcast_exact::<PyFloat>(vm).is_ok() {
                Ok(value.clone())
            } else {
                Err(vm.new_type_error(format!("must be real number, not {}", value.class().name)))
            }
        }
        "?" => Ok(vm.ctx.none()),
        "B" => {
            if value.clone().downcast_exact::<PyInt>(vm).is_ok() {
                Ok(vm.new_pyobj(u8::try_from_object(vm, value.clone()).unwrap()))
            } else {
                Err(vm.new_type_error(format!("int expected instead of {}", value.class().name)))
            }
        }
        "z" => {
            if value.clone().downcast_exact::<PyInt>(vm).is_ok()
                || value.clone().downcast_exact::<PyBytes>(vm).is_ok()
            {
                Ok(value.clone())
            } else {
                Err(vm.new_type_error(format!(
                    "bytes or integer address expected instead of {} instance",
                    value.class().name
                )))
            }
        }
        "Z" => {
            if value.clone().downcast_exact::<PyStr>(vm).is_ok() {
                Ok(value.clone())
            } else {
                Err(vm.new_type_error(format!(
                    "unicode string or integer address expected instead of {} instance",
                    value.class().name
                )))
            }
        }
        _ => {
            // "P"
            if value.clone().downcast_exact::<PyInt>(vm).is_ok()
                || value.clone().downcast_exact::<PyNone>(vm).is_ok()
            {
                Ok(value.clone())
            } else {
                Err(vm.new_type_error("cannot be converted to pointer".to_string()))
            }
        }
    }
}

impl PyValue for PySimpleType {
    fn class(_vm: &VirtualMachine) -> &PyTypeRef {
        Self::static_metaclass()
    }
}

#[pyimpl]
impl PySimpleType {
    #[pyslot]
    fn tp_new(cls: PyTypeRef, vm: &VirtualMachine) -> PyResult<PyRef<Self>> {
        match vm.get_attribute(cls.as_object().to_owned(), "_type_") {
            Ok(_type_) => {
                if vm.isinstance(&_type_, &vm.ctx.types.str_type)? {
                    if _type_.to_string().len() != 1 {
                        Err(vm.new_value_error("class must define a '_type_' attribute which must be a string of length 1".to_string()))
                    } else if !SIMPLE_TYPE_CHARS.contains(_type_.to_string().as_str()) {
                        Err(vm.new_attribute_error(format!("class must define a '_type_' attribute which must be\na single character string containing one of {}.",SIMPLE_TYPE_CHARS)))
                    } else {
                        PySimpleType {
                            _type_: _type_.downcast_exact::<PyStr>(vm).unwrap().to_string(),
                            value: AtomicCell::new(vm.ctx.none()),
                            __abstract__: vm
                                .isinstance(&cls.as_object(), PySimpleType::static_type())
                                .is_ok(),
                        }
                        .into_ref_with_type(vm, cls)
                    }
                } else {
                    Err(vm.new_type_error(
                        "class must define a '_type_' string attribute".to_string(),
                    ))
                }
            }
            Err(_) => {
                Err(vm.new_attribute_error("class must define a '_type_' attribute".to_string()))
            }
        }
    }

    #[pymethod(name = "__init__")]
    pub fn init(&self, value: Option<PyObjectRc>, vm: &VirtualMachine) -> PyResult<()> {
        match value.clone() {
            Some(ref v) if !self.__abstract__ => {
                let content = set_primitive(self._type_.as_str(), v, vm)?;
                self.value.store(content);
                Ok(())
            }
            Some(_) => Err(vm.new_type_error("abstract class".to_string())),
            _ => {
                self.value.store(match self._type_.as_str() {
                    "c" | "u" => vm.ctx.new_bytes(vec![0]),
                    "b" | "B" | "h" | "H" | "i" | "I" | "l" | "q" | "L" | "Q" => vm.ctx.new_int(0),
                    "f" | "d" | "g" => vm.ctx.new_float(0.0),
                    "?" => vm.ctx.new_bool(false),
                    _ => vm.ctx.none(), // "z" | "Z" | "P"
                });

                Ok(())
            }
        }
    }

    #[pyproperty(name = "value")]
    fn value(&self) -> PyObjectRef {
        unsafe { (*self.value.as_ptr()).clone() }
    }

    #[pyproperty(name = "value", setter)]
    fn set_value(&self, value: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
        let content = set_primitive(self._type_.as_str(), &value, vm)?;
        self.value.store(content);
        Ok(())
    }

    // From Simple_Type Simple_methods
    #[pymethod(name = "__ctypes_from_outparam__")]
    pub fn ctypes_from_outparam(&self) {}

    // From PyCSimpleType_Type PyCSimpleType_methods
    #[pyclassmethod]
    pub fn from_param(cls: PyTypeRef, vm: &VirtualMachine) {}

    // Simple_repr
    #[pymethod(name = "__repr__")]
    fn repr(zelf: PyRef<Self>) -> String {
        format!("{}({})", zelf.class().name, zelf.value().to_string())
    }

    // Simple_as_number
    // #[pymethod(name = "__bool__")]
    // fn bool(&self) -> bool {
    //
    // }
}
